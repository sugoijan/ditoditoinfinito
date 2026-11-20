## Multiplayer (a prototype behind a hidden switch,
## docs/plans/multiplayer-prototype-progress.md): its home page, the room
## a match of several songs is played in (practice against CPU opponents
## on this device for now), the room's Rules and More pop-ups, and song
## select when it chooses a stage's song.

## The multiplayer home: a centered column of wide buttons (about 380 px,
## about 30 Latin letters) under the screen's header, as the start menu's;
## the footer's right holds the account (sign in, or the name).

# The page's title beside its back icon on a phone (about 20 letters).
room-home-title = Multiplayer

# The page's name in its header, in small capitals: written in capitals
# where the language has them.
room-home-heading = MULTIPLAYER

# A button: automatic public rooms, not made yet: the button is grayed
# out.
room-home-find = Find match · Coming soon

# A button: a list of public rooms, not made yet: grayed out.
room-home-browse = Browse rooms · Coming soon

# A button: opens the practice room.
room-home-practice = CPU practice

# A button: makes an invite-only online room (asking to sign in first
# when signed out).
room-home-private = Private room

# A button: opens a page to paste an invite link into.
room-home-join = Join by link

# The sign-in dialog's title, asked for when something needs a sign-in.
room-sign-in-title = Sign in

# The sign-in dialog's text when Private room was chosen signed out
# (wrapped, about 60 letters a line).
room-sign-in-for-private = Private rooms are invite-only and are played as your Twitch identity. Sign in with Twitch to make one.

# The sign-in dialog's text when opened from the footer's Sign in.
room-sign-in-for-account = Sign in with Twitch to make private rooms and join invites. CPU practice needs no account.

# The join-by-link page: its title beside its back icon on a phone, and
# its name in its header, in small capitals.
room-join-title = Join by link
room-join-heading = JOIN BY LINK

## The room: the players at the left, the selected stage's song and chart
## in the middle, the match's stages as a list at the right, Ready and
## Start at the bottom right.

# The room's name in its header, in small capitals: written in capitals
# where the language has them.
room-practice-heading = CPU PRACTICE

# The heading over the players' column (about 16 letters).
room-players = Players

# Beside the players' heading: how many play, of how many may.
# Variables:
#   $players (Number) - players in the room, CPUs included
#   $cap (Number) - the most the room allows
room-players-count = { $players } / { $cap }

# The local player's name in the players' column (about 16 letters).
room-you = You

# A CPU opponent's name in the players' column.
# Variables:
#   $number (Number) - which CPU, from 1
room-cpu-name = CPU { $number }

# A bot's name wherever players are listed (the room, the stage's start
# screen, the standings, an invite's host): a simulated player an operator
# added to test a room, played by the CPU, labeled so that it is never
# taken for a person.
# Variables:
#   $name (String) - the bot's name
room-bot-name = { $name } (Bot)

# At the right of a CPU's row before the match starts: it takes its seat
# when the match starts (about 10 letters).
room-cpu-at-start = At start

# At the right of the player's row (about 10 letters each).
room-state-ready = Ready
room-state-not-ready = Not ready
# During a stage.
room-state-playing = Playing

# A stage's number on its row in the stage list (about 10 letters).
# Variables:
#   $number (Number) - the stage, from 1
room-stage = Stage { $number }

# A stage's row with no song yet.
room-stage-empty = No song yet

# The header's buttons.
room-rules = Rules

# Buttons: the stage's prompt's Ready, More's Stop match (about 16 letters each).
room-ready = Ready
room-unready = Cancel ready
room-stop-match = Stop match

# Feedback round AA: More's item (and its question's button) that ends an
# open-ended match now, between stages, with the songs played (about 20
# letters).
room-end-match = End the match now

# The question before it ends the match.
# Variables:
#   $count (Number) - the songs played in the match so far
room-end-match-body = { $count ->
    [one] The match ends now with the song played: its points decide it. The next match keeps these rules.
   *[other] The match ends now with the { $count } songs played: their points decide it. The next match keeps these rules.
}

# Round AC: on an open-ended match's stage's Ready card, for whoever may
# mark it, a row whose value (On or Off, the usual words) says whether
# this stage is the match's last song (about 16 letters).
room-ready-last-song = Last song

# More's items for an open-ended match's last song (about 24 letters each):
# the stage picked and not started made the match's last song, or not;
# and, before a pick, the next pick made the last song.
room-make-last = Make this the last song
room-not-last = Not the last song
room-pick-last-next = Make the next pick the last song

## The status line (over the list, or atop the room's panel): what the room waits for.

# The countdown to the stage's start.
# Variables:
#   $number (Number) - the stage, from 1
#   $seconds (Number) - whole seconds left
room-status-countdown = Stage { $number } starts in { $seconds }…

# The countdown to the stage's start where the seconds left are not
# known yet (the audience's page before the room's clock is).
# Variables:
#   $number (Number) - the stage, from 1
room-status-countdown-soon = Stage { $number } is about to start.

# The stage is being played (its play is over the room; seen only if
# it was left without a result).
# Variables:
#   $number (Number) - the stage, from 1
room-status-playing = Stage { $number } is being played.

# The stage's song is being read and checked on this device.
# Variables:
#   $number (Number) - the stage, from 1
room-status-preparing = Preparing stage { $number }'s song…

## Why something was refused, on the status line.

room-refused-no-song = Choose a song for this stage first.

room-refused-started = That stage has started; it cannot change now.

room-refused-phase = Not possible at this point of the match.

# Round AC: on the status line for a while after "Use this device" brought
# the room back to this device: that they are back, and whether they play
# the stage being readied or played now, or (it being under way without
# them) the one after it.
# Variables:
#   $plays (String) - "this" (they play stage $number), "next" (stage $number is under way without them: they play the next one) or "none" (nothing to say of a stage)
#   $number (Number) - the stage, from 1 (0 where $plays is "none")
room-back-here = { $plays ->
    [this] You're back in the room. You play stage { $number }.
    [next] You're back in the room. Stage { $number } is under way without you: you play the next one.
   *[none] You're back in the room.
}

room-refused-preparing = The song is still being prepared.

## Why a stage's song could not be prepared on this device, on the
## status line.

room-load-unreadable = The song could not be read. Choose another.

room-load-changed = The chart changed since it was chosen. Choose it again.

room-load-cpu = The CPUs cannot play this chart. Choose another.

# An online room's song could not be downloaded from the room's share.
room-load-download = The room's song could not be downloaded.

# Round P1b: the button at the right of that status line, once the
# game's own tries again are spent, while the stage can still be readied
# for (about 10 letters).
room-retry-load = Retry

# More's item doing the same (about 24 letters).
room-retry-download = Retry the download

# An online room that plays its stages live: this device worked out a
# different attack plan from the stage's chart than the room has (another
# version of the game, or another copy of the chart), so it sits the stage
# out.
room-load-plan = This device sees the stage's attacks differently from the room, so you sit this stage out.

# Variables:
#   $max (Number) - the most stages a match may have
room-rules-stage-count = A match has 1 to { $max } stages.

# Variables:
#   $cap (Number) - the most players, CPUs included
room-rules-over-cap = At most { $cap } players, CPUs included.

# Variables:
#   $min (Number) - the fewest players, CPUs included
room-rules-too-few = At least { $min } players, CPUs included.

room-rules-invalid = These rules cannot be used now.

# A refusal without a message of its own.
# Variables:
#   $code (String) - its name, as the room's model calls it
room-refused-other = Not possible now ({ $code }).

# Song select choosing a stage's song: the chart chosen is not one the
# CPUs can play.
# Variables:
#   $style (String) - the chart's style's name, as song select shows it
room-chart-unsupported = The CPUs play one-player charts with four lanes only, not { $style }. Choose another chart.

# The dialog's title saying so.
room-chart-unsupported-title = Not for CPUs

## Song select choosing a stage's song.

# The Enter key's hint on the host's song select in a room: picks the
# chart shown for the next stage, starting the match at the first (a word
# or three).
# Variables:
#   $number (Number) - the stage, from 1
room-pick-use = Pick for stage { $number }
# The same where some players don't have the song shown on their device
# yet: picking it is allowed, the room waits for them to load it.
# Variables:
#   $number (Number) - the stage, from 1
#   $count (Number) - players who don't have it yet
room-pick-use-missing = { $count ->
    [one] Pick for stage { $number } (one player doesn't have it yet)
   *[other] Pick for stage { $number } ({ $count } players don't have it yet)
}

## The Rules pop-up: this match's rules, locked while it runs, and the
## next match's, which can change.

room-rules-title = Rules

# The section of the rules the running match plays by.
room-rules-this-match = This match · locked

# The section of the rules the next match will play by.
room-rules-next-match = Next match

# Rows' labels (about 20 letters each).
room-rules-stages = Stages

# Feedback round AA: the Stages row's value for an open-ended match (no
# set count of songs), and its sentence under the row.
room-rules-stages-open = Open
# Variables:
#   $max (Number) - the most stages the room allows
room-rules-stages-open-about = Open: no set number of songs. A pick can be made the last song, or the match ended between songs; at most { $max }.

# The rule Joining during a match (On or Off) and its sentence.
room-rules-mid-match = Joining during a match
room-rules-mid-match-about = On: people who join while a match runs play from its next song not yet started, with no points for the songs they missed.
room-rules-cpus = CPU opponents
room-rules-level = Level
room-rules-points = Points

# Rows of the match's rules everyone plays under (DDI, this game's own
# ruleset): its timing difficulty (Easy, Normal, Hard, named as Options names
# them), Boos (a Boo is DDI's name for a press that hits no note, a
# judgment's name, kept in English) and attacks, each Off or On.
room-rules-difficulty = Timing difficulty
room-rules-boos = Boos
room-rules-attacks = Attacks

# The row under Attacks (shown while attacks are on) saying how many
# attacks may be on their way to one player at the same time (about 20
# letters), its value (a number, or "No limit") and a line under it.
room-rules-at-once = Attacks at once
# Variables:
#   $count (Number) - how many attacks: 1, 2 or 3
room-rules-at-once-count = { $count }
room-rules-at-once-no-limit = No limit
room-rules-at-once-about = How many attacks can be on their way to one player at the same time. They never overlap.

# The row under Attacks at once (shown while attacks are on) saying how
# many shocks one attack puts into the attacked player's field (about 20
# letters), its value (the number: 2, 4, 6 or 8) and a line under it.
room-rules-shocks = Shocks per attack
# Variables:
#   $count (Number) - how many shocks: 2, 4, 6 or 8
room-rules-shocks-count = { $count }
room-rules-shocks-about = How many shocks one attack puts into a player's field.

# The Attacks row's value in an online room, where the rule cannot be
# turned on yet (about 32 letters).
room-rules-attacks-online = Off · not in online rooms yet

# The row saying which charts of the song a stage is played on (about 20
# letters), and its two choices (one or two words each): the chart picked
# for everyone, or charts that adapt to each player, down to an easier
# chart of the same song when their life runs out and up to a harder one
# when they keep full life.
room-rules-charts = Charts

# Round AC (feedback round AA's rule, now under both chart rules): what
# running out of life does, the row's name (about 20 letters): Play on
# (the default) or Out; and a sentence for each under the row, saying
# what it does with the picked chart and with adaptive charts (where life
# running out first steps down to an easier chart).
room-rules-life-out = When life runs out
room-rules-life-out-play-on = Play on
room-rules-life-out-out = Out
room-rules-life-out-about-play-on = Play on: whoever runs out of life (with adaptive charts, on the easiest one) is failed, plays the song to its end and ranks under everyone who survived, the failed by their score.
room-rules-life-out-about-out = Out: running out of life (with adaptive charts, on the easiest one) ends the stage for them.
room-rules-charts-as-picked = As picked
room-rules-charts-adaptive = Adaptive

# A line under the Charts row saying what the chosen value does.
room-rules-charts-picked-about = Everyone plays the chart picked.
room-rules-charts-adaptive-about = Each player's chart changes with their skill.

# Added after that line where the song picked for the stage has no other
# chart in its style to move to.
room-rules-charts-no-ladder = The song picked has no other chart in its style: it plays as picked.

# The Charts row's value in a room that cannot adapt charts yet (an online
# room; about 32 letters).
room-rules-charts-online = As picked · not in online rooms yet

# The Points row's choices where the room offers them: how strongly a
# field of nine players or more favors its top places (one word each).
room-rules-points-strong = Strong
room-rules-points-medium = Medium
room-rules-points-mild = Mild

# A line under the Points row saying what the chosen value favors.
room-rules-points-strong-about = The top places earn the most; lower places in a large field earn smaller awards.
room-rules-points-medium-about = Between the two: the top places still lead, and lower places earn a little more.
room-rules-points-mild-about = Points reach furthest down a large field; the top places lead by less.

# Lines under the rows, muted.
room-rules-level-note = A CPU's stars show its level: the charts it keeps up with, not a skill rating.
# A note where the room offers the Points row's choice: the choice
# matters only with nine players or more.
room-rules-points-bias-note = The Points choice changes nothing with eight players or fewer.
room-rules-ruleset-note = Everyone plays the match under DDI with these rules, whatever their own options say.
room-rules-battle-note = The battle is provisional: your stage ends when your life runs out. With attacks, good timing fills a power bar, and a full bar sends an attack, a few shocks, into another player's field.
room-rules-locked-note = Changes apply to the next match; this one keeps its rules.
room-rules-clears-note = Applying clears readiness.

# The buttons that lower and raise a number (one sign each).
room-rules-less = −
room-rules-more = +

# The buttons that choose the previous and the next value of a rule (one
# sign each).
room-rules-previous = ‹
room-rules-next = ›

# The button that saves the changes.
room-rules-apply = Apply

# The Rules pop-up's title while a match is running: its rows are the
# next match's (the running match keeps its rules).
room-rules-title-next = Rules for the next match

# The first line at the top of the Rules pop-up while a match is running;
# the running match's rules follow on the next line (as
# room-rules-locked-line writes them), which cannot change.
# Variables:
#   $number (Number) - the stage the match is on, from 1
#   $stages (Number) - the match's stages (0: an open-ended match, no set count)
room-rules-running = { $stages ->
    [0] This match is on stage { $number } and keeps these rules:
   *[other] This match is on stage { $number } of { $stages } and keeps these rules:
}

# The line under those: the pop-up's rows are for the next match. Where a
# row's value differs from the running match's, a small dot (●) marks it,
# which the sentence explains (keep the dot).
# Variables:
#   $marked (String) - "yes" when some row is marked, else "no"
room-rules-running-next = { $marked ->
    [yes] The rows below are for the next match. ● marks what differs from this one.
   *[no] The rows below are for the next match.
}

# A button beside Apply while a match is running (about 20 letters):
# saves the rules and stops the running match, so that the next song
# picked starts a new match under them.
room-rules-apply-restart = Apply and restart

# The question it asks first: its title, its sentence, and the button
# that does it (about 16 letters; Cancel is the other).
room-rules-restart-title = Restart the match?
room-rules-restart-body = This match stops now, and its points so far won't carry over. The next song picked starts a new match under these rules.
room-rules-restart-confirm = Restart match

## Changing a running match's rules from a later stage on (where the
## room's authority takes such a change): the Rules pop-up's edits go to
## the running match from a stage not begun yet, or to the next match.

# The Rules pop-up's title while its rows change the running match's rules
# from a later stage on.
# Variables:
#   $number (Number) - the stage the changes apply from, from 1
room-rules-title-from = Rules from stage { $number }

# The first line at the top of the Rules pop-up while a match is running
# in a room whose rules can change from a later stage on; the rules of the
# stage the match is on follow on the next line (as room-rules-locked-line
# writes them).
# Variables:
#   $number (Number) - the stage the match is on, from 1
#   $stages (Number) - the match's stages (0: an open-ended match, no set count)
room-rules-running-stage = { $stages ->
    [0] This match is on stage { $number }, under these rules:
   *[other] This match is on stage { $number } of { $stages }, under these rules:
}

# The line under those where the pop-up's rows change the running match's
# rules from a later stage on (the stage named: one nobody has readied or
# loaded for yet). Where a row's value differs from that stage's rules, a
# small dot (●) marks it, which the sentence explains (keep the dot).
# Variables:
#   $number (Number) - the stage the changes apply from, from 1
#   $marked (String) - "yes" when some row is marked, else "no"
room-rules-running-from-points = { $marked ->
    [yes] Timing, Boos, zero life, attacks and Points below apply from stage { $number } on. ● marks what you changed.
   *[no] Timing, Boos, zero life, attacks and Points below apply from stage { $number } on.
}

# A row at the top of the Rules pop-up while a match is running: where its
# changes go. Its label (about 16 letters), then one of its two values
# (about 16 letters each).
room-rules-destination = Changes apply
# Variables:
#   $number (Number) - the stage the changes apply from, from 1
room-rules-destination-stage = From stage { $number }
room-rules-destination-next = Next match

# A quiet note under the rows while the changes apply from a later stage:
# the rows that never change during a match ("Apply and restart" is the
# button of room-rules-apply-restart).
room-rules-stage-only-note-points = Stages and CPUs can't change during a match: choose Next match for them, or Apply and restart.

# A quiet sentence after the Points row's own (room-rules-points-*-about)
# where Points from a later stage differs from the stage before: the
# match's stages will have been awarded on different curves, so its
# totals shift ("Apply and restart" is the button of
# room-rules-apply-restart, offered when $restart is "yes").
# Variables:
#   $number (Number) - the stage the changes apply from, from 1
#   $before (String) - the Points of the stage before: "strong", "medium" or "mild"
#   $restart (String) - "yes" where Apply and restart is offered, else "no"
room-rules-points-shift = Stages before stage { $number } keep { $before ->
    [medium] Medium
    [mild] Mild
   *[strong] Strong
} points, so the match's totals will add stages awarded on different curves.{ $restart ->
    [yes] {" "}For a close contest, use Apply and restart.
   *[no] {""}
}

# The room's status line once the rules were changed from a later stage
# on, until that stage begins: what changes, then the line it would say
# otherwise.
# Variables:
#   $number (Number) - the stage the change applies from, from 1
#   $rules (String) - what changes, the parts below joined ("Hard timing · Boos · No attacks")
#   $then (String) - the status line it would say otherwise (a sentence)
room-status-rules-change = From stage { $number }: { $rules }. { $then }
# The same where the status line is about an earlier stage than the one
# the change applies from (the stage being picked comes first): the line
# it would say otherwise, then the change after a dot, as the room's other
# additions to its line are (the audience's count).
# Variables:
#   $then (String) - the status line it would say otherwise (a sentence)
#   $number (Number) - the stage the change applies from, from 1
#   $rules (String) - what changes, the parts joined ("Hard timing · Boos · No attacks")
room-status-then-rules-change = { $then } · From stage { $number } on: { $rules }

# The parts of such a change, each a rule as it becomes: the timing
# difficulty ("Hard timing": never a chart's difficulty), Boos on or off,
# attacks on or off, what zero life does.
# Variables:
#   $difficulty (String) - "easy", "normal" or "hard"
room-rules-change-timing = { $difficulty ->
    [easy] Easy timing
    [hard] Hard timing
   *[normal] Normal timing
}
# Variables:
#   $boos (String) - "on" or "off"
room-rules-change-boos = { $boos ->
    [on] Boos
   *[off] No Boos
}
# Variables:
#   $attacks (String) - "on" or "off"
room-rules-change-attacks = { $attacks ->
    [on] Attacks
   *[off] No attacks
}
# Variables:
#   $at_once (String) - "one", "two", "three" or "no_limit": how many attacks may be on their way to one player at once
room-rules-change-at-once = { $at_once ->
    [one] One attack at once
    [two] Two attacks at once
    [no_limit] No limit on attacks at once
   *[three] Three attacks at once
}
# Variables:
#   $shocks (String) - "two", "four", "six" or "eight": how many shocks one attack has
room-rules-change-shocks = { $shocks ->
    [two] Two shocks per attack
    [six] Six shocks per attack
    [eight] Eight shocks per attack
   *[four] Four shocks per attack
}
# Variables:
#   $charts (String) - "adaptive" or "as_picked"
room-rules-change-charts = { $charts ->
    [adaptive] Adaptive charts
   *[as_picked] Charts as picked
}
# Variables:
#   $points (String) - "strong", "medium" or "mild"
room-rules-change-points = { $points ->
    [medium] Medium points
    [mild] Mild points
   *[strong] Strong points
}
# Two of those parts (or a part and the parts joined before) in a list.
# Variables:
#   $first (String) - the parts before
#   $second (String) - the next part
room-rules-change-join = { $first } · { $second }

# Refused: a change of the running match's rules made for a stage that
# has begun since (someone picked its song or started it first).
room-refused-rules-later = The stage these changes were for has begun since. Check where they apply now, then Apply again.

# Refused: no stage of the running match is left for a change.
room-refused-rules-no-stage = No stage of this match is left to change: the changes can be for the next match.

# Beside the Standings tab's heading where the stage was played under
# rules changed during the match (keep the dot).
# Variables:
#   $rules (String) - the stage's rules in a few words, as room-rules-words writes them ("DDI · Hard timing · Boos")
room-table-rules-changed = ● Changed rules: { $rules }

# Under the match's standings' title, a line for each stage from which the
# rules changed during the match (keep the dot: the stage columns played
# under other rules than the match began with are marked with it).
# Variables:
#   $number (Number) - the stage the change applied from, from 1
#   $rules (String) - those rules in a few words, as room-rules-words writes them
room-results-rules-changed = ● From stage { $number }: { $rules }

## The More pop-up.

# Leaves the room for the multiplayer home, forgetting the practice.
room-leave = Leave room

## The room's host (2026-11-12). When its host goes, an online room is
## passed on to someone else or closes, by a setting of the room's that
## its host chooses. The host can make other players co-hosts (who help
## run the room) and hand the room over to one of them ("Make host").
## These are in the "Host options" pop-up, opened from the Players
## pop-up by the host alone.

# A button at the bottom of the Players pop-up (the host's alone, about
# 14 letters), and the title of the pop-up it opens.
room-host-options = Host options

# A row's label in Host options (about 22 letters): what the room does
# when its host leaves it or stays disconnected. Its value is one of the
# two below, changed with the arrows beside it.
room-host-leaves = If the host leaves

# Its two values (about 16 letters each).
room-host-leaves-pass = Pass it on
room-host-leaves-close = Close the room

# What each value does, in one sentence: under the row in Host options,
# and as a quiet line in everyone's Players pop-up (wrapped, about 60
# letters a line).
room-host-leaves-pass-about = If the host leaves, a co-host becomes the host, or else the player who joined first.
room-host-leaves-close-about = If the host leaves, the room closes for everyone.

# The heading over the other players' rows in Host options (small, about
# 20 letters): each row has the buttons below.
room-host-people = Other players

# In place of the rows when the host is alone in the room.
room-host-nobody = Nobody else is here yet.

# Buttons on another player's row in Host options (about 14 letters
# each): make them a co-host, make a co-host an ordinary player again,
# and hand the room over to them (asked once more first).
room-make-co-host = Make co-host
# The same button's place on the row of someone who came by the room's
# watch-only link: lets them choose Play.
room-let-play = Let play
# Under the name of someone who came by the watch-only link and may not
# play until let (Host options).
room-host-watch-only = Watch-only link
room-remove-co-host = Remove co-host
room-make-host = Make host

# The title of the question asked before the room is handed over.
# Variables:
#   $name (String) - the player's display name
room-make-host-title = Make { $name } the host?

# Its text (wrapped, about 60 letters a line): what happens. The button
# that does it is room-make-host.
# Variables:
#   $name (String) - the player's display name
room-make-host-body = { $name } will run the room and you will become a co-host. Only the new host can make you the host again.

## Asking before leaving a room (2026-11-12): a dialog titled "Leave the
## room?" with a sentence or two saying what leaving does in the case at
## hand, and two buttons, Stay (chosen first) and Leave. It is shown only
## where leaving costs the others or the player something: to an online
## room's host; to someone playing, choosing the songs or co-hosting
## while a match is running; in CPU practice with a match running. Back,
## More's Leave room, closing the window and quitting the game all ask
## it. The texts are wrapped, about 60 letters a line.

# The dialog's title.
room-leave-title = Leave the room?

# Its first button (about 8 letters): nothing changes.
room-leave-stay = Stay

# Its second button (about 8 letters): leaves the room.
room-leave-confirm = Leave

# To the room's host, where the room is passed on to another player.
# Variables:
#   $name (String) - the display name of the player who becomes the host
room-leave-host-pass = You are the host. { $name } will become the host.

# The same while a match is running.
# Variables:
#   $name (String) - the display name of the player who becomes the host
room-leave-host-pass-match = You are the host. { $name } will become the host, and the match goes on without you.

# To the room's host, where the room's setting says it closes when the
# host leaves (others are in it).
room-leave-host-close = You are the host. The room will close for everyone.

# The same while a match is running.
room-leave-host-close-match = You are the host. The room will close for everyone, and the match in progress will end.

# To the room's host with nobody else in the room.
room-leave-host-alone = You are the only one here. The room will close.

# To a player in the running match (not the host).
room-leave-playing = You are in the match in progress. It goes on without you, and you score nothing for the stages you miss.

# To the player choosing the room's songs while a match is running (not
# the host, not playing in it).
room-leave-picking = You are choosing the songs. The host will choose them instead, and the match goes on without you.

# To a co-host while a match is running (not playing in it, not choosing
# the songs).
room-leave-co-host = You are a co-host. The match in progress goes on without you.

## A stage's play: the CPU opponents' small fields beside the field,
## each with its name and its level's stars above it, and Out once its
## life is gone (narrow: a few letters each).

# A CPU whose life is gone: out of this stage.
room-play-out = Out

# Feedback round AA: on a miniature field (and a followed player's), a
# competitor whose life ran out under "When life runs out: Play on":
# failed, still playing (about 8 letters).
room-play-failed = Failed
# Under the player's own life gauge, the same of themselves.
room-play-failed-on = Failed: playing on

# Once the player has won the stage before the song's end (they finish
# the song): in the line under the opponents' small fields at the right.
room-play-won = Stage won

# Before "Stage won": the player is the only one still in, but shocks
# already sent against them are still to come, so the stage is not won
# yet (they could still run out of life and share first). In the line
# under the opponents' small fields at the right, until the stage is won.
room-play-last-standing = Last standing

# A large field's ladder, under the small fields at the left, beside a
# strip of everyone's life: the player's place among everyone in the
# stage, of how many, and how many are still in (narrow: about 24 letters).
# Variables:
#   $place (Number) - the player's place now, from 1
#   $field (Number) - everyone in the stage, the player included
#   $left (Number) - those whose life has not run out
room-play-ladder = { $place } of { $field } · { $left } still in

# The same where the one watching has no place of their own (watching
# every field).
# Variables:
#   $field (Number) - everyone in the stage
#   $left (Number) - those whose life has not run out
room-play-ladder-field = { $left } of { $field } still in

# The same on a spectator's grid that shows its fields a page at a time,
# by standing: which page, of how many.
# Variables:
#   $page (Number) - the page shown, from 1
#   $pages (Number) - the pages
#   $field (Number) - everyone in the stage
#   $left (Number) - those whose life has not run out
room-play-ladder-page = Page { $page } of { $pages } · { $left } of { $field } still in

## A battle's moment no announcer line said, for a few seconds: in the
## line under the opponents' small fields at the right (narrow: about 22
## letters).

# The first stage of a match begins.
room-play-cue-match-begin = Match begins

# A later stage begins.
# Variables:
#   $number (Number) - the stage's number, from 2
room-play-cue-stage-begin = Stage { $number } begins

# The match's last stage begins: a counted match's last stage, or an open
# match's song marked as its last.
room-play-cue-final-stage = Final stage

# The player's life is about to run out.
room-play-cue-life-critical = Life critical

# The players still in the stage became this many.
# Variables:
#   $count (Number) - how many are left (4 or 2)
room-play-cue-survivors = { $count } players left

# The player's life ran out: out of this stage.
room-play-cue-eliminated = You're out

# The player's life ran out and they step down to an easier chart of the
# song (a match's rule), named as the song list names it.
# Variables:
#   $chart (String) - the chart's name ("Medium")
room-play-cue-step-down = Stepping down: { $chart }

# The player kept full life long enough and steps up to a harder chart of
# the song (a match's rule), named as the song list names it.
# Variables:
#   $chart (String) - the chart's name ("Challenge")
room-play-cue-step-up = Stepping up: { $chart }

# The player's life ran out on a harder chart a second time in this stage
# (a match's rule "Charts: Adaptive"), so they are not stepped up to it
# again: they stay on the chart named, said once, quietly, in the play's
# status line where "Stepping down: …" would be.
# Variables:
#   $chart (String) - the chart's name, as the song list names it ("Hard")
room-play-cue-stay = Staying on { $chart }

# The player and others went out last together: first place shared.
room-play-cue-stage-drawn = Stage drawn

# An opponent's full power bar was spent on an attack against the
# player: its shocks are on their way into the player's field.
room-play-cue-attack-incoming = Attack incoming

# The player's own full power bar was spent on an attack against an
# opponent.
room-play-cue-attack-launched = Attack launched

## A battle's attacks during a stage (where the match's Attacks rule is on).

# An attack policy's name: how a player's full power bar picks whom to
# attack. Shown beside each player in the room, on a stage's start
# prompt and, small, by the power bar during the play, where Tab, a
# controller's View button or a tap changes it. Spread: whoever has been
# attacked least (short, one word).
room-attack-policy-spread = Spread

# Retaliate: whoever attacked the player last (short, one word).
room-attack-policy-retaliate = Retaliate

# Top score (the policy the code calls Leader; named so since round DB,
# a host read "Leader" as leading the room): whoever has the most life
# (short, two words at most).
room-attack-policy-leader = Top score

# A small muted line under the life gauge over the field (its right
# end), while the player's full bar waits: nobody can be attacked right
# now, or no safe moment in the song is near (about 20 letters). The
# player's policy stands by itself at the gauge's left end.
room-play-attack-waiting = Waiting for target

# On a stage's start prompt, where the match has attacks: the label of
# the row where the player chooses their attack policy, followed by
# ◀ Spread ▶ (about 12 letters).
room-targeting = Targeting

# After a key cap (Shift) or a controller's button under the Targeting
# row on a stage's start prompt: that key changes the policy while the
# song plays (two or three words).
room-targeting-during-play = during play

# One line beside or under that row: what the chosen policy does (up to
# about 55 letters). Spread.
room-attack-policy-spread-about = Your attacks go to whoever has been attacked least.

# Retaliate.
room-attack-policy-retaliate-about = Your attacks go to whoever attacked you last.

# Top score.
room-attack-policy-leader-about = Your attacks go to whoever has the most life.

# A note on a stage's results: the last player left was knocked out by
# an attack while the stage waited for it, so first place is shared.
room-results-drawn-final-attack = Drawn: final attack

# On a stage's start prompt, in the line of the play's modifiers: the
# player's scroll speed is so slow that attacks could not warn in time,
# so this stage plays it faster.
room-prompt-speed-raised = Speed raised so attacks can warn in time

## A stage's start prompt, results and the room between stages.

# On a stage's start prompt, in the line of the play's modifiers: the
# stage and against how many CPUs (what running out of life does follows).
# Variables:
#   $number (Number) - the stage, from 1
#   $stages (Number) - the match's stages (0: an open-ended match, no set count)
#   $cpus (Number) - CPU opponents
room-prompt-stage-cpus = { $stages ->
    [0] Stage { $number }
   *[other] Stage { $number } of { $stages }
} · { $cpus ->
    [one] against one CPU
   *[other] against { $cpus } CPUs
}

# The same for an online room's stage against other people alone.
# Variables:
#   $number (Number) - the stage, from 1
#   $stages (Number) - the match's stages (0: an open-ended match, no set count)
#   $others (Number) - the other players starting it
room-prompt-stage-people = { $stages ->
    [0] Stage { $number }
   *[other] Stage { $number } of { $stages }
} · { $others ->
    [one] against one other player
   *[other] against { $others } other players
}

# The same for an online room's stage against other people and the
# room's CPUs.
# Variables:
#   $number (Number) - the stage, from 1
#   $stages (Number) - the match's stages (0: an open-ended match, no set count)
#   $others (Number) - the other players starting it
#   $cpus (Number) - CPU opponents
room-prompt-stage-mixed = { $stages ->
    [0] Stage { $number }
   *[other] Stage { $number } of { $stages }
} · against { $others ->
    [one] one other player
   *[other] { $others } other players
} and { $cpus ->
    [one] one CPU
   *[other] { $cpus } CPUs
}

# After the stage's line on its start prompt: what running out of life
# does, where the charts are as picked (the player is out of the stage).
room-prompt-life-out = out when your life runs out

# In its place where the match's rule When life runs out is Play on
# (round AC): the player is failed and plays the song on to its end.
room-prompt-life-play-on = failed but playing on when your life runs out

# In its place where the match's charts are Adaptive: what the rule
# means for the player, in a few words (the chart they play changes,
# easier or harder, with how well they play; how and when is not said
# here, nor that life running out is what moves it down).
room-prompt-chart-adapts = the chart changes with your skill

# The same for a stage this device watches (a member who chose Watch, or
# someone watching with the room's audience): nothing is played here.
# Variables:
#   $number (Number) - the stage, from 1
#   $stages (Number) - the match's stages (0: an open-ended match, no set count)
#   $players (Number) - the players starting it
room-prompt-stage-watched = { $stages ->
    [0] Stage { $number }
   *[other] Stage { $number } of { $stages }
} · watching { $players ->
    [one] one player
   *[other] { $players } players
}

# A stage's results: the button that goes back to the room (where Song
# list is after a single play; about 16 letters).
room-results-back = Back to room

# A stage's prompt: the rules everyone plays the match under, after the
# stage's line.
# Variables:
#   $rules (String) - the rules in a few words, as room-rules-words writes them ("DDI · Hard timing · Boos · No attacks")
room-prompt-ruleset = Rules: { $rules }

# The match's rules in a few words, where they are summarized (the Rules
# pop-up's locked line, a stage's prompt): the ruleset's name, then only
# what differs from the usual: the timing difficulty when it is not Normal
# ("Hard timing", "Easy timing": never a chart's difficulty), Boos
# when they are on, and that there are no attacks when the match has
# them turned off (they are on by default), the Points choice when it
# is not the default Strong, adaptive charts when they are chosen, and
# how many attacks may be on their way to one player at once when it is
# not the default three, and how many shocks one attack has when it is
# not the default four; and (round AC) out of the stage when life runs
# out, a match with no set number of songs, and no joining during a
# match, each only where it is not the usual.
# Variables:
#   $ruleset (String) - the ruleset's name ("DDI")
#   $difficulty (String) - "easy", "normal" or "hard"
#   $boos (String) - "on" or "off"
#   $attacks (String) - "off" when the match's attacks are turned off, else "on"
#   $points (String) - "strong", "medium" or "mild"
#   $charts (String) - "as_picked" when everyone plays the chart picked (the charts not adapting to each player, which is the usual), else "adaptive"
#   $at_once (String) - "one", "two" or "no_limit" when attacks are on and that is how many may be on their way to one player at once, else "three"
#   $shocks (String) - "two", "six" or "eight" when attacks are on and that is how many shocks one attack has, else "four"
#   $life_out (String) - "out" when a player whose life runs out is out of the stage, else "play_on" (they play on as failed, the usual)
#   $stages (String) - "open" when the match has no set number of songs, else "counted" (the usual)
#   $joining (String) - "off" when people who come during a match wait for the next match, else "on" (they play its next song, the usual)
room-rules-words = { $ruleset }{ $difficulty ->
    [easy] {" "}· Easy timing
    [hard] {" "}· Hard timing
   *[normal] {""}
}{ $boos ->
    [on] {" "}· Boos
   *[off] {""}
}{ $attacks ->
    [off] {" "}· No attacks
   *[on] {""}
}{ $points ->
    [medium] {" "}· Medium points
    [mild] {" "}· Mild points
   *[strong] {""}
}{ $charts ->
    [as_picked] {" "}· Charts as picked
   *[adaptive] {""}
}{ $life_out ->
    [out] {" "}· Out when life runs out
   *[play_on] {""}
}{ $at_once ->
    [one] {" "}· One attack at once
    [two] {" "}· Two attacks at once
    [no_limit] {" "}· No limit on attacks at once
   *[three] {""}
}{ $shocks ->
    [two] {" "}· Two shocks per attack
    [six] {" "}· Six shocks per attack
    [eight] {" "}· Eight shocks per attack
   *[four] {""}
}{ $stages ->
    [open] {" "}· Open stages
   *[counted] {""}
}{ $joining ->
    [off] {" "}· No joining during a match
   *[on] {""}
}

# A note on a stage's results and under the song in the room: how the
# player did, by how the stage ended for them.
# Variables:
#   $number (Number) - the stage, from 1
#   $place (Number) - the place, from 1 (tied players share it)
#   $field (Number) - how many played the stage
#   $points (String) - the points it gave, as a number ("3", "2.5")
room-mark-survived = Stage { $number }: place { $place } of { $field } · +{ $points } pts

# The same, when the player's life ran out during the stage.
# Variables:
#   $number (Number) - the stage, from 1
#   $place (Number) - the place, from 1 (tied players share it)
#   $field (Number) - how many played the stage
#   $points (String) - the points it gave, as a number ("3", "2.5")
room-mark-out = Stage { $number }: out of life · place { $place } of { $field } · +{ $points } pts

# The same, when the player's life ran out under "When life runs out:
# Play on" and they played on to the song's end as
# failed (ranked after everyone who survived, by score).
# Variables:
#   $number (Number) - the stage, from 1
#   $place (Number) - the place, from 1 (tied players share it)
#   $field (Number) - how many played the stage
#   $points (String) - the points it gave, as a number ("3", "2.5")
room-mark-failed = Stage { $number }: failed, played on · place { $place } of { $field } · +{ $points } pts

# The same, when the player left the stage while playing (no points).
# Variables:
#   $number (Number) - the stage, from 1
#   $place (Number) - the place, from 1
#   $field (Number) - how many played the stage
room-mark-forfeit = Stage { $number }: left the stage · place { $place } of { $field } · no points

# A player's place and points in the players' column during a match.
# Variables:
#   $rank (Number) - their place in the match so far
#   $points (String) - their points, as a number ("3", "2.5")
room-rank-points = #{ $rank } · { $points } pts

# Match points as a number, where the room's points show (a share of a
# tied place's points comes out a fraction: 1.5, 9.33).
# Variables:
#   $points (Number) - the points, to hundredths
room-points-value = { $points }

## The match's results, in the room where the song was.

room-results-title = Match results

# An interrupted match: only its finished stages counted.
room-results-stopped = Match stopped · finished stages only

# Who won (names are the players' own; the local player is "You").
# Variables:
#   $count (Number) - how many share first place
#   $names (String) - their names, separated by commas
room-results-winners = { $count ->
    [one] Winner: { $names }
   *[other] Winners: { $names }
}

# The table's headings: the total and each stage, short ("S1").
room-results-total = Total
# Variables:
#   $number (Number) - the stage, from 1
room-results-stage = S{ $number }

# A stage not played by someone (sat out or not reached), in the table.
room-results-none = —

## The footer's key hints (a word or two each).

room-hint-move = Move
room-hint-back = Back

## Online rooms (invite-only, signed in with Twitch): the account on the
## multiplayer home, signing in, an invite's page before joining, and what
## the room says of its connection. Twitch is the sign-in service's name.

# On the home, the account line: who is signed in.
# Variables:
#   $name (String) - the signed-in person's display name
room-account-signed-in = Signed in as { $name }

# On the home, the account line when nobody is signed in.
room-account-signed-out = Not signed in. CPU practice needs no account.

# On the home while the game asks the server who is signed in.
room-account-checking = Checking who is signed in…

# On the home when the server did not answer.
room-account-unreachable = Couldn't reach the server. CPU practice still works.

# Buttons (about 24 letters each).
room-sign-in = Sign in with Twitch
room-sign-out = Sign out

# The join-by-link page: a field to paste an invite link into, its
# label and its hint (shown while empty).
room-invite-field = Invite link
room-invite-field-hint = Paste an invite link

# The button beside the invite field.
room-invite-go = Open invite

# Under the invite field when what was pasted is not an invite.
room-invite-not-one = That isn't an invite link.

# Under the invite field when what was pasted is a room's page address
# (…#/mp/room/…) rather than its invite link (feedback round AA).
room-invite-room-address = That's a room's page address, not its invite link. Ask the room's host for the invite link.

# Desktop, while signing in: the browser is open on Twitch's page.
room-sign-in-waiting = Finish signing in with Twitch in your browser, then come back here.

# Desktop, while signing in, above the line before: the code the page the
# browser opened asks for (the server's page asks the person to type it,
# so a sign-in link nobody started on this computer can't sign them in).
# Variables:
#   $code (String) - six letters and digits, grouped: ABC-DEF
room-sign-in-code = Type this code in the page your browser opened: { $code }

# Desktop, after the browser's page took too many wrong codes and ended
# the sign-in. Try again (room-sign-in-retry) starts again with a new code.
room-sign-in-too-many-codes = Too many wrong codes were typed in the browser, so that sign-in ended. Try again for a new code.

# Buttons while signing in on desktop (about 24 letters each).
room-sign-in-again-browser = Open the browser again
room-sign-in-cancel = Cancel

# While the game reads who signed in.
room-sign-in-completing = Signing in…

# After signing in was canceled or denied.
room-sign-in-canceled = Sign-in canceled.

# After signing in failed.
# Variables:
#   $code (String) - the server's reason, as a code
room-sign-in-failed = Sign-in couldn't complete ({ $code }).

# A button after a canceled or failed sign-in.
room-sign-in-retry = Try again

# While a private room is being made.
room-creating = Making the room…

# When making the room failed.
# Variables:
#   $code (String) - the server's reason, as a code
room-create-failed = Couldn't make the room ({ $code }).

# When making the room was refused because the server's operator has
# paused new rooms for now (a sentence on the multiplayer home's status
# line; about 60 letters).
room-create-paused = New rooms are paused for now. Try again later.

# The same where the server already holds as many rooms as it can.
room-create-busy = The server has as many rooms as it can hold right now. Try again later.

# The same where this person already hosts as many rooms as one person
# may at once (rooms they made and have not closed).
room-create-too-many = You already host as many rooms as one person can at once. Close one, or try again later.

## An invite's page (opened from an invite link): the room it is for
## before signing in, and joining it.

# The page's title beside its back icon on a phone (about 20 letters).
room-invite-title = Invite

# The page's name in its header, in small capitals.
room-invite-heading = INVITE

# While the invite is read.
room-invite-reading = Reading the invite…

# The room's host.
# Variables:
#   $name (String) - the host's display name
room-invite-host = Hosted by { $name }

# How many people are in the room.
# Variables:
#   $count (Number) - people in the room
room-invite-members = { $count ->
    [one] One person in the room
   *[other] { $count } people in the room
}

# What joining now means (one of these).
room-invite-open = You can join now. You'll choose Ready yourself.
room-invite-next-match = A match is running: you'll watch and play from the next match.
# Feedback round AA: in its place where the running match seats newcomers
# for its next song not yet started (joining during a match).
room-invite-next-song = A match is running: you can play from its next song.
room-invite-closed = The host has closed this room to new players.
room-invite-full = This room is full.
# In the place of room-invite-open or room-invite-next-match where every
# player's place is taken: joining is still possible, to watch.
room-invite-full-watch = The room is full: you'll join to watch.
# In the same place on the room's watch-only link's page.
room-invite-watch = You're invited to watch.

# Why an invite does not admit (one of these).
room-invite-expired = This invite has expired. Ask the host for a new link.
room-invite-rotated = This invite was replaced. Ask the host for the new link.
room-invite-unknown = This invite doesn't exist.
room-invite-room-closed = This room has closed.
room-invite-unreachable = Couldn't reach the server.

# Another reason, as the server's code.
# Variables:
#   $code (String) - the server's reason
room-invite-refused = Couldn't join ({ $code }).

# Buttons (about 24 letters each).
room-invite-join = Join room
# The same button on a watch-only link's page.
room-invite-watch-join = Watch
room-invite-sign-in = Sign in with Twitch to join

# While joining.
room-invite-joining = Joining…

## The online room: its connection on the status line, the people's
## roles and connections in the players' column, and More's invite
## actions.

# The status line while the room is first reached.
room-link-joining = Joining the room…

# The status line when the connection dropped and the game tries again.
room-link-reconnecting = Connection lost. Reconnecting…

# The status line when the session ended (sign in again to return).
room-link-sign-in-again = Your sign-in ended. Sign in again to return to the room.

# The status line when the room closed.
room-link-closed = This room has closed.

# Round P1b: the status line, and the notice's title, when this device
# stopped because the same person opened the room on another device or
# tab (the server keeps four a person and closed the one connected
# longest). This device no longer reconnects by itself.
room-link-opened-elsewhere = This room was opened on another device.

# The notice's text under that title.
room-opened-elsewhere-body = The room is open on four other devices or tabs of yours. Use this device to play here: the one connected longest stops instead.

# The button (the notice's, and More's item) that connects this device
# again, stopping another the same way (about 20 letters).
room-use-this-device = Use this device

# The notice's other button: it closes the notice and leaves this device
# stopped (about 12 letters).
room-opened-elsewhere-not-now = Not now

# The status line (and the audience's page, and over a watched stage)
# when the server's operator closed the room: said to its members and to
# everyone watching.
room-closed-by-operator = The room was closed.

# Said (on an invite's page, or on the room's status line) where the
# room's server is newer than this game: desktop's, which needs a newer
# build of the game.
room-update-required = This room needs a newer version of the game.
# The same on the web, where reloading the page brings the newer game.
room-update-reload = The game was updated. Reload to play in this room.
# The button that reloads the page (the web; about 12 letters).
room-update-reload-button = Reload
# Said (on an invite's page, the audience's page, or on the room's status
# line) where the room's server is older than this game: it is being
# updated, and the game tries again by itself on the room's line.
room-server-updating = The server is being updated. Try again in a moment.

# The status line after leaving.
room-link-left = You left this room.

# Feedback round AA: the status line, and the notice's title, on a room's
# page opened by someone not signed in here (the room's page address
# shared in place of its invite link, or a reload after the sign-in
# ended): never "Your sign-in ended", which is for a session that did
# exist. Signing in returns to the room where they are its member.
room-link-sign-in-to-enter = Sign in to enter this room.

# The notice's text under that title.
room-sign-in-to-enter-body = Rooms are entered with their invite link. If you're already in this room, sign in with Twitch to return to it; if not, ask its host for the invite link.

# The status line, and the notice's title, on a room's page opened by
# someone signed in who never joined it (no invite link): the page's
# address lets nobody in.
room-link-not-a-member = You're not in this room. Ask its host for the invite link.

# The notice's text under that title.
room-not-a-member-body = A room's page address doesn't let anyone in; its invite link does. Ask the host to send it to you (the room's More, Copy invite link).

# A button (those notices', and More's item in Leave room's place there)
# that goes to the multiplayer home (about 16 letters).
room-multiplayer-home = Multiplayer home

# Under a person's name in the players' column (about 16 letters each).
room-role-host = Host
room-role-co-host = Co-host
room-role-admin = Admin
room-watching = Watching
room-next-match = Next match
# Feedback round AA: a player's state in the players' rows for someone
# who joined during a match that seats them at its next song; for one
# waiting because every place is taken (CPUs too); and, under the name,
# the stage someone who joined during the match played from (about 16
# letters each).
room-next-song = Next song
room-place-full = No place free
# Variables:
#   $number (Number) - the stage, from 1
room-joined-at = From stage { $number }

# At the right of a person's row: their connection (about 12 letters).
room-conn-joining = Joining
room-conn-reconnecting = Reconnecting
room-conn-away = Away

# A stage's row and the song's card when another person chose the song
# and this device has not read it from the room yet.
room-host-song = Song chosen by the host

# The status line while a stage's song is being shared with the room (by
# the host's device; the others wait for it).
# Variables:
#   $number (Number) - the stage, from 1
room-status-sharing = Stage { $number }'s song is being shared with the room…

# The status line on the device sharing the song, as it uploads.
# Variables:
#   $number (Number) - the stage, from 1
#   $percent (Number) - how much is sent, 0 to 100
room-status-uploading = Sharing stage { $number }'s song with the room… { $percent }%

# The status line when this device could not share the stage's song; More
# has Retry sharing.
# Variables:
#   $number (Number) - the stage, from 1
#   $reason (String) - why, a short sentence or the server's code
room-status-share-failed = Couldn't share stage { $number }'s song: { $reason } Retry from More.

# The status line when the room checked the shared song and the chosen
# chart was not in it.
# Variables:
#   $number (Number) - the stage, from 1
room-status-share-mismatch = The song shared for stage { $number } doesn't have the chosen chart.

# The status line when the room could not use a stage's shared song.
# Variables:
#   $number (Number) - the stage, from 1
room-status-share-unavailable = Stage { $number }'s song couldn't be shared with the room.

# More's item to share a stage's song again after a failure (about 24
# letters).
room-retry-sharing = Retry sharing

# The status line while an online stage is played: each player plays it on
# their own device and the room ranks what each reports; the others are
# not shown during the play in this prototype.
# Variables:
#   $number (Number) - the stage, from 1
room-status-online-stage = Stage { $number } is being played. The results come once everyone has finished.

# A chart the library has no digest for yet cannot be chosen online.
room-chart-no-digest = This chart can't be chosen online yet. Try again once the library has read it.

# More's items (about 24 letters each).
room-copy-invite = Copy invite link
# A second link to the room: whoever comes by it watches, and plays only
# once the host or a co-host lets them.
room-copy-watch-invite = Copy watch-only link
room-replace-invite = Replace invite link
# The question asked before the invite is replaced (it cannot be undone):
# its title, then what it does. The button that does it is
# room-replace-invite.
room-replace-invite-title = Replace the invite link?
room-replace-invite-body = The old invite link stops working, and so do the room's other links made from it (watch-only and audience). Everyone already in the room stays.
room-sign-in-again = Sign in again

# The status line after More's invite items. The first also tells the
# host, quietly, that the room does not outlive its people: a room with
# nobody connected closes after a couple of minutes (one line, about 70
# letters).
room-invite-copied = Invite link copied. The room stays open while someone is in it.
room-watch-invite-copied = Watch-only link copied.
room-invite-replaced = Invite replaced: the old link no longer works.
room-invite-not-replaced = Couldn't replace the invite.

# The status line for a while after joining a room whose player places
# were all taken: the player joined to watch. "Play" is the Play/Watch
# control's name (room-intent-play).
room-joined-to-watch = The room is full: you're watching. Choose Play when a place frees up.

## The match flow (M7b): song select is the room. The host picks each
## stage's song there; the others follow the host's song.

# The status line over the list (the host's) or at the top of the room's
# panel (the others'): one line.
room-status-pick-first = Pick a song to start the match.

# The status line of an online room's host while nobody else is in the
# room and no song is picked yet: the way to bring people in (feedback
# round AA: a host shared the page's own address, which lets nobody in).
room-status-alone-invite = Pick a song to start the match. To bring people in, share the invite link from More, not this page's address.

# Variables:
#   $number (Number) - the stage, from 1
room-status-pick-next = Pick stage { $number }'s song.

# A stage's start screen gathers readiness: the stage starts once everyone
# is ready, or when the time is up (whoever is not ready then sits it out).
# Variables:
#   $number (Number) - the stage, from 1
#   $seconds (Number) - whole seconds left
room-status-gathering = Stage { $number } starts once everyone is ready · { $seconds } s

# Before a stage's Ready countdown: the room waits for players still
# loading the stage's song (downloading or preparing it), up to the time
# shown; then the countdown starts and whoever is still loading may join
# if they finish in time.
# Variables:
#   $count (Number) - how many players are still loading
#   $seconds (Number) - whole seconds left of the wait
room-status-loading-wait = { $count ->
    [one] Waiting for 1 player to load · { $seconds } s
   *[other] Waiting for { $count } players to load · { $seconds } s
}

# Round AC: before a stage starts, the room waits a while for someone
# whose download of the stage's song failed and is being tried again
# (up to the time shown; then it starts without them).
# Variables:
#   $name (String) - the first such player's name
#   $more (Number) - how many others are trying again too (often 0)
#   $seconds (Number) - whole seconds left of the wait
room-status-retry-wait = { $more ->
    [0] Waiting for { $name }: trying their download again · { $seconds } s
    [one] Waiting for { $name } and 1 other: trying their downloads again · { $seconds } s
   *[other] Waiting for { $name } and { $more } others: trying their downloads again · { $seconds } s
}

# For someone who is not in the stage the others play, where no reason
# below applies (they chose Watch, for one), then what comes next.
# Variables:
#   $number (Number) - the stage, from 1
#   $then (String) - what comes next for them, one of the room-sat-out-then messages below
room-status-sitting-out = You're sitting out stage { $number }; the others are playing it. { $then }

# Why the local player sits a stage out, then what comes next, on the
# room's status line and under the song on the stage's start screen (two
# short sentences each; keep each about this long). Here: they had not
# pressed Ready when the stage started.
# Variables:
#   $number (Number) - the stage, from 1
#   $then (String) - what comes next for them, one of the room-sat-out-then messages below
room-status-sat-out-not-ready = You weren't ready when stage { $number } started. { $then }

# The stage's song was still loading on their device when it started.
# Variables:
#   $number (Number) - the stage, from 1
#   $then (String) - what comes next for them, one of the room-sat-out-then messages below
room-status-sat-out-loading = Stage { $number }'s song was still loading here when it started. { $then }

# The stage's song could not be loaded on their device.
# Variables:
#   $number (Number) - the stage, from 1
#   $then (String) - what comes next for them, one of the room-sat-out-then messages below
room-status-sat-out-failed = Stage { $number }'s song couldn't be loaded here. { $then }

# The room had lost their connection when the stage started.
# Variables:
#   $number (Number) - the stage, from 1
#   $then (String) - what comes next for them, one of the room-sat-out-then messages below
room-status-sat-out-away = Your connection was lost when stage { $number } started. { $then }

# What comes next for the player sitting a stage out, the second sentence
# of the lines above. They are shown the stage on this device and play the
# next one:
room-sat-out-then-watch-next = You're watching this stage and play the next one.
# Shown the stage; it is the match's last.
room-sat-out-then-watch-last = You're watching this stage, the match's last.
# Shown the stage; under this match's rules a missed stage ends the match
# for them.
room-sat-out-then-watch-out = You're watching this stage; missing it ends the match for you.
# Shown the stage; they chose Watch, and play again once they choose Play.
room-sat-out-then-watch-chosen = You're watching this stage; choose Play to play the next one.
# The same where the stage is not shown on this device: they play the next one.
room-sat-out-then-next = You play the next one.
# Not shown; it is the match's last stage.
room-sat-out-then-last = It's the match's last stage.
# Not shown; missing it ends the match for them.
room-sat-out-then-out = Missing it ends the match for you.
# Not shown; they chose Watch.
room-sat-out-then-chosen = Choose Play to play the next one.

# For someone who joined the room while its match was running, during a
# stage: they are not in this match.
# Variables:
#   $number (Number) - the stage, from 1
room-status-waiting-next-match = Stage { $number } is being played. You joined during this match and play from the next one.

# Feedback round AA: the same, where the match seats people who join
# during it from its next song.
# Variables:
#   $number (Number) - the stage being played, from 1
room-status-waiting-next-stage = Stage { $number } is being played. You joined during this match and play from its next song.

# The status line of someone who wants to play while every place in the
# match is taken (its CPUs' too).
room-status-waiting-place = Every place in this match is taken, its CPUs' too. You play from the next match, or sooner if a place frees.

# Before the status line's own words where the stage it is about is an
# open-ended match's last song.
# Variables:
#   $line (String) - the status line
room-status-last-song = Last song · { $line }

# On the room's status line and a stage's Ready card, for a player who
# was not in the stage when it started but may still enter it (round
# P1b's late entry: within a few seconds of the song's start, where the
# room plays live). Ready then plays it from where the song is.
# Variables:
#   $number (Number) - the stage, from 1
room-status-enter-late = Stage { $number } has started. Press Ready now to play it from here.

room-status-pick-new-match = Match over. Pick a song to start the next match.

room-status-stopped-pick = Match stopped. Pick a song to start the next match.

# The same where the next match has other rules than the stopped one (as
# after the Rules pop-up's Apply and restart).
room-status-stopped-new-rules = Match stopped. Pick a song to start the next match under the new rules.

## One picker at a time: one player chooses the room's songs (its host at
## first). A host or co-host who follows can take the choosing over at
## once; everyone else follows the song that player's song select rests
## on.

# The status line of everyone who follows while another player chooses
# the match's first song: one line.
# Variables:
#   $name (String) - the display name of the player who chooses
room-status-picker-first = { $name } is choosing the first song.

# The same between stages.
# Variables:
#   $name (String) - the display name of the player who chooses
#   $number (Number) - the stage, from 1
room-status-picker-next = { $name } is choosing stage { $number }'s song.

# The same after a match ended or was stopped.
# Variables:
#   $name (String) - the display name of the player who chooses
room-status-picker-new-match = Match over. { $name } is choosing the next match's first song.

# The status line when nobody chooses the songs (the player who did is
# gone and so is the host), for a player who cannot take the choosing
# over.
room-status-nobody-picks = Nobody is choosing songs right now.

# The same for a host or co-host, who can: it names the Pick songs
# button (room-take-pick; keep the two alike).
room-status-nobody-picks-take = Nobody is choosing songs. Use Pick songs to choose them.

# A button in the header of a following host's or co-host's song select,
# an item of the room's More menu and a key's hint in the footer (about
# 12 letters): takes the choosing of songs over at once; the player who
# chose until then follows.
room-take-pick = Pick songs

# After a person's role in the players' lists ("Host · Picking"): this
# player chooses the room's songs now (a word, about 8 letters).
room-role-picking = Picking

# Said in a pop-up when a host or co-host tried to choose a song while
# another player chooses.
room-refused-not-picker = Another player is choosing the songs now.

# A CPU's personality, after its level under its name: what kind of
# patterns it is good at (a word or two each).
room-cpu-style-balanced = Balanced
room-cpu-style-stream = Streams
room-cpu-style-jacks = Jacks
room-cpu-style-chords = Chords
room-cpu-style-stamina = Stamina
room-cpu-style-technical = Technical
# Good at getting around a battle's shocks (a dodger).
room-cpu-style-evasive = Evasive
# Prone to stepping on a battle's shocks (a stomper).
room-cpu-style-reckless = Reckless
# Multipliers of its own, no preset's.
room-cpu-style-custom = Custom

# At the right of a player's row (about 12 letters each).
room-state-loading = Loading
# Round AC: their download of the song failed and is being tried again
# (the room waits for it a while).
room-state-retrying = Retrying download
# Variables:
#   $percent (Number) - how much of the song is loaded, 0 to 100
room-state-loading-percent = Loading { $percent }%
room-state-load-failed = Load failed
room-state-sitting-out = Sitting out
# At the right of a player's row once a song is picked for the stage and
# until their device says it is loading it (the song may still be on its
# way to the room): before Loading, and never "Not ready" (about 16
# letters).
room-state-waiting = Waiting for song
# At the right of the row of a player who sits the stage out, with why
# (about 20 letters each). Not ready when it started:
room-state-sat-out-not-ready = Sat out: not ready
# Their song was still loading when it started.
room-state-sat-out-loading = Sat out: loading
# Their song failed to load.
room-state-sat-out-failed = Sat out: load failed
# Their connection was lost when it started ("away", as room-conn-away).
room-state-sat-out-away = Sat out: away
# Why a player's song did not load, in the players panel on the room's
# song select, at the right of their line (about 16 letters each): the
# room's song could not be downloaded; it could not be read; its chart is
# not the one chosen.
room-state-failed-download = Download failed
room-state-failed-unreadable = Song unreadable
room-state-failed-changed = Chart changed
# The same place: their device worked out the stage's attacks differently
# from the room (they sit the stage out).
room-state-failed-plan = Attacks differ

# A phone's one-line strip of the players on the room's song select: after
# the initials that fit, how many more players there are.
# Variables:
#   $count (Number) - players not shown
room-strip-more = +{ $count }

# The player's result of a stage in the picks' list, short.
# Variables:
#   $place (Number) - the place, from 1
#   $field (Number) - how many played the stage
#   $points (String) - the points it gave, as a number ("3", "2.5")
room-mark-short = { $place } of { $field } · +{ $points } pts
# The same for a stage the player left.
# Variables:
#   $place (Number) - the place, from 1
#   $field (Number) - how many played the stage
room-mark-short-forfeit = { $place } of { $field } · left

# The heading over the match's picks in the room's panel (about 16
# letters).
room-picks-heading = Picks

# Beside it: how many stages have a song, of the match's.
# Variables:
#   $picked (Number) - stages with a song
#   $stages (Number) - the match's stages (0: an open-ended match, no set count)
room-picks-count = { $stages ->
    [0] { $picked }
   *[other] { $picked } of { $stages }
}

# Marks at the top and the bottom of a scrolled list: how many rows are
# hidden there (a tap scrolls).
# Variables:
#   $count (Number) - rows hidden
room-panel-more-above = ▲ { $count } more
# Variables:
#   $count (Number) - rows hidden
room-panel-more-below = ▼ { $count } more
# In the room's More menu on song select's wide layout: folds the players
# panel at the screen's right to a narrow rail of state icons (L does the
# same), and opens it again.
room-panel-fold = Fold the players
room-panel-unfold = Show the players

# Refusals said on the status line or in a pop-up.
room-refused-too-few = Not enough players to start a stage.
room-refused-host-only = Only the host can do that.
# A pop-up's title saying why something was refused.
room-refused-title = Not possible

# The Rules pop-up as it opens by itself on entering: applies and closes
# (short).
room-rules-ok = OK

# The running match's rules, in one line.
# Variables:
#   $stages (Number) - the match's stages (0: an open-ended match, no set count)
#   $cpus (Number) - its CPU opponents
#   $rules (String) - its rules in a few words, as room-rules-words writes them ("DDI · Hard timing · Boos")
room-rules-locked-line = { $stages ->
    [0] Open stages
    [one] One stage
   *[other] { $stages } stages
} · { $cpus ->
    [one] one CPU
   *[other] { $cpus } CPUs
} · { $rules }

room-rules-no-cpus = No CPU opponents.

# The button removing a CPU from the next match (a mark).
room-rules-remove = ×

# Buttons under the CPUs (about 16 letters each): one more CPU, drawn at
# random; or chosen (by level, several at once, or by name).
room-rules-add-random = Add random
room-rules-add-custom = Add custom…

## Add custom: CPUs by level, or one by name.

room-add-cpus-title = Add CPUs

room-add-cpus-count = How many

# The button adding CPUs at the level chosen (the row above shows the
# level as stars, never as a number).
# Variables:
#   $count (Number) - how many
room-add-cpus-add = { $count ->
    [one] Add one CPU
   *[other] Add { $count } CPUs
}

# The heading over the CPUs listed by name, each added by choosing it.
room-add-cpus-by-name = Or one by name

room-add-cpus-full = Every seat is taken.

room-add-cpus-none-left = Every CPU is seated already.

## A stage's standings, after its results.

# Variables:
#   $number (Number) - the stage, from 1
room-standings-title = Stage { $number } standings

# Closes the standings: back to song select (about 12 letters).
room-standings-continue = Continue

## The stage's start screen in a room: Ready instead of Start.

# Under the song: the room canceled the stage's start (too few were ready,
# or the host picked again).
room-prompt-canceled = The stage's start was canceled.

# Feedback round O: once the player is ready, one press no longer takes
# it back. Under the Ready button, which then says "Cancel ready" (a
# short line, about 34 letters): holding the button (or Enter, Space, a
# controller's confirm) or pressing it twice cancels.
room-prompt-unready-hint = Hold or press twice to cancel

# Round P1b: the Ready button's label (and the footer's Enter hint) once
# an online room's start is committed with the player in it: the
# countdown to the stage runs and Ready can no longer be taken back
# (about 16 letters). In CPU practice the button still says "Cancel
# ready", which cancels the countdown.
room-prompt-starting = Starting…

# The same line after Back (Esc, the Back button, a controller's Back) was
# pressed once while ready: Back leaves this stage for good, so it too
# takes a hold or a second press (about 34 letters).
room-prompt-leave-hint = Hold or press twice to leave

# The footer's key hints while ready, after Enter and after Esc (or a
# controller's buttons): what each does, then how (a few words each).
room-prompt-footer-unready = Cancel ready: hold or press twice
room-prompt-footer-leave = Leave: hold or press twice

## A stage is due while the player is on a screen opened over the room
## (Options, Calibrate, the announcer studio): a notice over that screen,
## with the room's status line (its countdown) under these words. The
## screen is never closed for them.

# The notice's title (about 30 letters). On the web also the browser
# tab's title while the tab is in the background and the stage waits for
# the player's Ready.
# Variables:
#   $number (Number) - the stage, from 1
room-due-title = Stage { $number } is starting
# For a player who can play the stage.
room-due-play = Your room's next stage is starting. Go to Ready to play it.
# For a stage they are not playing, which they can watch.
room-due-watch = A stage you are not playing is starting. You can watch it.
# The notice's buttons (about 16 letters each): to the stage's Ready
# screen, to the stage watched, or staying on the screen.
room-due-go = Go to Ready
room-due-go-watch = Watch
room-due-stay = Stay here

## A stage's results: the Standings and Graphs tabs where a play's four
## graphs are (feedback round B).

# The two tabs over the results' right side (about 12 letters each).
room-results-tab-standings = Standings
room-results-tab-graphs = Graphs

# The footer's hint for Left and Right on a stage's results: they change
# the tab (about 14 letters).
room-results-hint-tabs = Change tab

# The Standings tab's heading: which stage of the match these are.
# Variables:
#   $number (Number) - the stage's number, from 1
#   $stages (Number) - the match's stages (0: an open-ended match, no set count)
room-table-title = { $stages ->
    [0] Stage { $number }
   *[other] Stage { $number } of { $stages }
}

# Beside the heading while the room has not confirmed the stage: some
# players are still playing or have not sent their result (about 36
# letters).
room-table-waiting = Waiting for everyone's results…

# The table's column headings, in small letters (about 8 letters each):
# the player; the points the stage awarded; the match total after it; the
# place in the match after it; the ruleset's score or percent; the grade;
# life at the end, or when they went out; the longest combo; the counts
# of each judgment, drawn as a bar.
room-table-player = Player
room-table-award = Stage
room-table-total = Total
room-table-match = Match
room-table-score = Score
room-table-grade = Grade
room-table-life = Life
room-table-combo = Combo
room-table-judgments = Judgments

# The points a stage awarded a player, in their row.
# Variables:
#   $points (String) - the points, as a number ("3", "2.5")
room-table-award-value = +{ $points }

# In a player's row while their result has not come (about 16 letters).
room-table-playing = Still playing…

# A row's life column for a player whose life ran out: when in the song.
# Variables:
#   $time (String) - the song's minute and second ("1:02")
room-table-out = Out { $time }

# The same for a player whose life ran out under "When life runs out:
# Play on": failed then, and played on to the end
# (feedback round AA).
# Variables:
#   $time (String) - the song's minute and second ("1:02")
room-table-failed = Failed { $time }

# A row's life column for a player who left the stage (about 6 letters).
room-table-left = Left

# Beside a player's name in the table where they stepped down to an easier
# chart of the song during the stage (a match's rule): the chart they
# finished on, named as the song list names it, after a down arrow.
# Variables:
#   $chart (String) - the chart's name ("Easy")
room-table-stepped = ↓ { $chart }

# The same where they finished on a harder chart than the stage's (they
# stepped up), after an up arrow.
# Variables:
#   $chart (String) - the chart's name ("Challenge")
room-table-stepped-up = ↑ { $chart }

# Three small columns at the table's end where the match has attacks,
# a number under each (their headings as the others': capitals small,
# very short: 4 to 6 letters). The attacks this player launched.
room-table-sent = Sent

# The attacks launched against this player.
room-table-taken = Taken

# The shocks (an attack's hazards) this player stepped on.
room-table-shocks = Shocks

## Under the standings on the local player's own results, where the
## stage had attacks: their battle in a few short lines (small, muted),
## joined by a dot where they fit on one line.

# Whom their attacks went to.
# Variables:
#   $list (String) - the opponents' names, with a count where more than one ("Mossy Accordion ×2, Tinsel Pike")
room-summary-sent = Attacks sent: { $list }

# They launched none.
room-summary-sent-none = No attacks sent

# Who attacked them.
# Variables:
#   $list (String) - the opponents' names, with a count where more than one ("Basalt Wren ×2")
room-summary-taken = Attacks taken: { $list }

# Nobody attacked them.
room-summary-taken-none = No attacks taken

# The shocks they stepped on.
# Variables:
#   $count (Number) - how many
room-summary-shocks = Shocks stepped on: { $count }

# The heading over the match's result on the last stage's Standings (in
# small letters, about 16).
room-final-heading = Match result

## Watching (a member chooses for themselves to play the stages or only
## to watch them; a watcher takes no seat and has no place in the
## standings).

# An item of the room's More menu and a button at the foot of the
# Players pop-up (about 12 letters), shown while the player plays:
# from now on they only watch the stages.
room-intent-watch = Watch only

# The same control while the player watches: they play the stages
# again.
room-intent-play = Play

# On the status line: a song was picked but every person in the room
# has chosen to watch, so there is nobody to play the stage.
room-refused-all-watching = Everyone is watching. Someone has to play.

# On the status line when a watcher chose Play and every player's place
# is taken. "Play" is the control's name (room-intent-play).
room-refused-players-full = Every player's place is taken. Choose Play again when one frees up.

# On the status line when someone who came by the room's watch-only link
# chose Play (only an older screen offers it).
room-refused-watch-only = You came by a watch-only link: the host or a co-host can let you play.

## A stage watched: everyone's fields over the whole screen, or one
## player's field large in the middle with the others beside it.

# In the line under the fields while all of them show, for the first
# seconds of a stage: how to follow one player.
room-watch-hint = Tab or tap a field to follow one player

# The same where the fields come in pages (a large stage): also how to see
# the next page (Page Up and Page Down on a keyboard, up and down on a
# controller's D-pad; a tap on the line beside it).
room-watch-hint-pages = Tab or tap to follow · Page Up/Down or D-pad for more

# At the start of the same status line under the opponents' fields, during
# an online stage played live, once the room's server lost the stage's live
# part (it restarted): the play goes on, the others' fields stop moving, and
# the run is reported at its end as before.
room-live-lost = The room lost the live stage; your run counts at the end

# In the same line while one player's field is the large one in the
# middle.
# Variables:
#   $name (String) - the player followed
room-watch-following = Watching { $name }

# After that line's words, beside a small key cap (Tab, or a controller's
# shoulder button): what the key does while one player is followed, the
# next player's field becomes the large one (quiet, about 12 letters).
room-watch-next = next player

# On the field of the player followed, under their life gauge, after
# their name: their place among everyone in the stage now (narrow: about
# 8 letters).
# Variables:
#   $place (Number) - their place now, from 1
#   $field (Number) - everyone in the stage
room-watch-place = { $place } of { $field }

# In the line under the fields, for a few seconds: a player's life ran
# out.
# Variables:
#   $name (String) - who
room-watch-cue-out = { $name } is out

# The stage is decided: this player won it.
# Variables:
#   $name (String) - who
room-watch-cue-won = { $name } wins the stage

# Someone watching: this competitor is the only one still in, with shocks
# against them still to come (the stage is not won until they pass).
# Variables:
#   $name (String) - the competitor's name
room-watch-cue-last-standing = { $name } is last standing

# One player attacks another (shocks are sent into the other's notes).
# Variables:
#   $from (String) - who attacks
#   $to (String) - who is attacked
room-watch-cue-attack = { $from } attacks { $to }

# On a touch screen, in the line under the fields while a stage is
# watched, quietly (after "Watching [name]" while one player is followed,
# else alone): a tap on a field follows that player.
room-watch-tap-hint = Tap a field to follow it

## The player's own field after they are out of a stage, which they go
## on watching to its end.

# Over the player's own dimmed field after they quit the stage's play:
# they left the stage (as "Out" is shown where their life ran out; the
# standings say "left" for it too, room-mark-short-forfeit).
room-play-forfeit = Left the stage

# Under the field while the player watches the rest of a stage they are
# out of: the key (or button) that ends the watching and shows the
# stage's results.
# Variables:
#   $key (String) - the key's or button's name ("Enter", "Start")
room-play-skip = { $key }: skip to results

# The same on a touch screen, a button to tap (about 16 letters).
room-play-skip-tap = Skip to results

# The first sentence of the "Leave the room?" question when it is asked
# over a stage the player has just left by closing the game or going
# back; what leaving the room does follows it on a line of its own.
room-leave-forfeited = You left this stage: no points for it.

## A stage's results for someone who watched it (no play of their own).

# The small heading over the stage's winner, where a player's own
# judgments would be (about 16 letters).
room-watched-winner = Stage winner

# In its place when first place was shared.
room-watched-winners = Stage winners

# Under the winner: the viewer took no part.
room-watched-note = You watched this stage.

## The audience (docs/plans/multiplayer-audience.md): people who watch a
## room beyond its members, counted and not listed, by a link of their
## own. "Audience" is everyone who watches that way; the room's own
## members who watch are not part of it.

# How many watch with the room's audience: on the invite's page, the
# audience's page and Host options.
# Variables:
#   $count (Number) - people watching with the audience
room-audience-watching = { $count ->
    [one] One person watching
   *[other] { $count } watching
}
# Host options' line when the host has closed the audience.
room-audience-closed-short = Audience closed
# The room's status line (song select, a stage's prompt) with how many
# watch after it, between stages.
# Variables:
#   $status (String) - the room's status line, as it would read alone
#   $count (Number) - people watching with the audience
room-status-with-audience = { $count ->
    [one] { $status } · One watching
   *[other] { $status } · { $count } watching
}
# More's item (about 24 letters): copies the link that lets anyone watch
# with the audience (they never join the room).
room-copy-audience-invite = Copy audience link
# The status line after it.
room-audience-invite-copied = Audience link copied: anyone with it can watch.
# Host options: the audience's row, its label alone or with how many watch.
room-host-audience = Audience
# Variables:
#   $state (String) - how many watch, or that it is closed (room-audience-watching, room-audience-closed-short)
room-host-audience-line = Audience · { $state }
# Host options' button (about 20 letters): sends everyone watching with
# the audience away and lets nobody in.
room-audience-close = Close the audience
# The same button once closed.
room-audience-open = Open the audience
# The status line after the host closes or opens it.
room-audience-closed-notice = The audience is closed: nobody watches with it now.
room-audience-opened-notice = The audience is open again.

# The invite page on an audience link, where the room's own says whether
# one can join.
room-invite-audience = You're invited to watch with the audience.
# In its place on the room's own link when every member's place is taken.
room-invite-full-audience = The room is full: you can watch with its audience.
# The button there (about 24 letters).
room-invite-audience-join = Watch with the audience
# While the audience join is under way, and on the audience's page before
# the room is shown.
room-audience-joining = Joining the audience…

# Why a seat in the audience was refused or ended.
room-audience-full = The room's audience is full.
room-audience-closed = The host closed the audience.
room-audience-too-many-seats = You are already watching this room on two devices.
# The same for someone watching without signing in: the audience's seats
# are counted by network (a household, a campus, a phone carrier's), eight
# a network.
room-audience-too-many-network = Too many people on your network are already watching this room.
# Too many joins from one network within a minute; the button can be
# pressed again shortly.
room-audience-busy = Many people on your network joined just now. Try again in a moment.
# The same for someone signed in: their own account joined the audience
# too many times within a minute.
room-audience-busy-account = You joined this room's audience many times just now. Try again in a moment.
room-audience-link-unknown = This audience link isn't this room's, or it was replaced.
room-audience-none = This room has no audience to watch with.

# Under that sentence, said over a watched stage whose seat ended (the
# audience closed, the room ended, the link replaced): the key that goes
# back to the audience's page (about 20 letters: it is on a small plate).
# Variables:
#   $key (String) - the key's or button's name ("Enter")
room-audience-ended-back = { $key }: back
# The same on a touch screen, a button to tap (about 16 letters).
room-audience-ended-back-tap = Back

# The audience's page: its title (window) and heading (header, small caps).
room-audience-title = Watching
room-audience-heading = Audience
# Its Back.
room-audience-leave = Stop watching
# Its button while a stage is being played that this device is not
# watching (it arrived, reloaded or came back during it): into the stage
# as it stands (about 20 letters).
room-audience-watch-stage = Watch the stage
# What the room is doing, between stages.
room-audience-doing-lobby = The host is choosing the first song.
# Variables:
#   $number (Number) - the stage, from 1
room-audience-doing-picking = Stage { $number }: a song is being chosen.
# Variables:
#   $number (Number) - the stage, from 1
#   $song (String) - the song's title and artist (data)
room-audience-doing-picked = Stage { $number }: { $song }. It starts once the players are ready.
# A room whose stages are not streamed (an older server): only its
# results come.
# Variables:
#   $number (Number) - the stage, from 1
room-audience-doing-not-live = Stage { $number } is being played. This room's stages can't be watched; their results come here.
room-audience-doing-match-over = The match is over.
room-audience-doing-stopped = The match was stopped.
# This device worked out the stage's attacks differently from the room:
# the fields are drawn without the shocks.
room-audience-plan-mismatch = This device sees the stage's attacks differently from the room: the fields are drawn without shocks.
room-audience-song-unreadable = The stage's song couldn't be read here: its results come when the stage ends.
# The last stage's winner.
# Variables:
#   $number (Number) - the stage, from 1
#   $names (String) - who placed first (data, comma-separated)
room-audience-stage-won = Stage { $number } won by { $names }
room-audience-standings = Standings
# One standing.
# Variables:
#   $rank (Number) - the place, from 1
#   $name (String) - the player's name (data)
#   $points (String) - their match points, as a decimal
room-audience-standing = { $rank }. { $name } · { $points } points
# The players' heading before any standing.
# Variables:
#   $count (Number) - players of the match
room-audience-players = { $count ->
    [one] One player
   *[other] { $count } players
}

# Refused (in the Rules pop-up, or on the room's status line): someone else
# changed the rules while this player's edit was open; nothing of theirs
# was applied and the pop-up now shows the rules as they are.
room-refused-rules-changed = Someone else changed the rules meanwhile. Here they are now: check them, then Apply again.

## The room's song list (room model revision 17): an online room's song
## select has two tabs over its list, the room's own songs (the same for
## everyone, added by its hosts) and the player's own library, from which
## a host adds songs to the room.

# The song select's tab of the room's song list, and More's item that
# shows it (about 14 letters).
room-tab-room-songs = Room songs
# The tab of the player's own songs, and More's item that shows it (about
# 14 letters).
room-tab-my-songs = My songs
# In the list's place when the room's list is empty, to who may add songs
# (a host).
room-songs-empty-adder = No songs in the room yet. Add songs from My songs.
# The same, to anyone else in the room.
room-songs-empty-member = The host hasn't added any songs yet.
# A room song's state for this player, in its row's line: being uploaded by
# whoever added it.
room-song-uploading = Uploading
# The same, this player's own upload, how far.
# Variables:
#   $percent (Number) - how much is uploaded, 0 to 100
room-song-uploading-percent = Uploading { $percent }%
# The upload failed (or what was uploaded was not the song added).
room-song-failed = Upload failed
# The song is on this device (in its own library, or read already).
room-song-ready = On this device
# The song is being read onto this device for a stage.
room-song-loading = Loading
# Not on this device yet: it is read when the song is picked.
room-song-not-loaded = Not loaded yet
# Being downloaded onto this device ahead of the picks, how much is
# there (a song's state in its row's line).
# Variables:
#   $percent (Number) - how much is downloaded, 0 to 100
room-song-downloading-percent = Downloading { $percent }%
# The same before its size is known.
room-song-downloading = Downloading
# A song's state followed by how many of the room's players have it on
# their device, said while not everyone does.
# Variables:
#   $state (String) - its state for this player (one of the states above)
#   $have (Number) - players who have it
#   $of (Number) - the room's players
room-song-state-have = { $state } · { $have } of { $of } have it
# Who added a room song, in its row's line.
room-song-added-by-you = Added by you
# Variables:
#   $name (String) - who added it (data)
room-song-added-by = Added by { $name }
# A room song's row's line: its artist, its state and who added it.
# Variables:
#   $artist (String) - the song's artist (data)
#   $state (String) - its state (one of the room-song- states above)
#   $by (String) - who added it (room-song-added-by…)
room-song-line = { $artist } · { $state } · { $by }
# The same for a song without an artist.
# Variables:
#   $state (String) - its state
#   $by (String) - who added it
room-song-line-no-artist = { $state } · { $by }
# The same for the song the one who chooses the room's songs is looking at
# (it may be picked next), said first.
# Variables:
#   $artist (String) - the song's artist (data)
#   $state (String) - its state
#   $by (String) - who added it
#   $picker (String) - who chooses the room's songs (data)
room-song-line-spotlight = { $picker } is on it · { $artist } · { $state } · { $by }
# The Enter key's hint on My songs for a host: adds the song shown to the
# room's list (a few words).
room-add-song-use = Add to the room
# The same where the song is in the room's list already (nothing happens).
room-song-in-room = In the room
# More's item for a room song this player may remove.
room-remove-song = Remove from the room
# More's item for a room song whose upload from this device failed.
room-retry-upload = Upload again
# Refused: the song is in the room's list already.
room-refused-song-listed = That song is in the room already.
# Refused: the room's list holds as many songs as it may and refuses new
# ones (its host's setting), or every song in it is in use.
room-refused-list-full = The room's song list is full.
# Refused: the person added as many songs as one may.
# Variables:
#   $limit (Number) - how many one person may add
room-refused-too-many-songs = { $limit ->
    [one] You can add one song to this room.
   *[other] You can add up to { $limit } songs to this room.
}
# Refused: a stage that is picked or under way plays that song.
room-refused-song-in-use = That song is picked for a stage, so it stays for now.
# Refused: the song is still being uploaded.
room-refused-song-not-ready = That song is still being uploaded.
# Refused: the song (or its chart) is not in the room's list any more.
room-refused-song-gone = That song isn't in the room's list any more.
# Refused: the song can't be listed as it is (too many charts, say).
room-refused-song-invalid = That song can't be added to the room.

## Host options: the room's song list's size and what an addition to a full
## list does (the owner's settings).

# The row's label (about 24 letters).
room-host-list-size = Room songs at most
# The row's value.
# Variables:
#   $count (Number) - the list's size, 1 to 100
room-host-list-size-value = { $count ->
    [one] One song
   *[other] { $count } songs
}
# The row's label (about 24 letters).
room-host-when-full = When the list is full
# Its choices: the oldest song not in use is removed to make room; or new
# songs are refused until one is removed.
room-host-when-full-remove = Remove the oldest
room-host-when-full-refuse = Refuse new songs
# The note under the two rows: what they mean. Hosts and co-hosts may
# fill the whole list; anyone else (once a room lets everyone add songs)
# half of it.
# Variables:
#   $count (Number) - how many songs anyone but a host may add (half the size)
room-host-list-note = { $count ->
    [one] Hosts and co-hosts may fill the whole list; anyone else may add one song. A song stays until it is removed or the room ends.
   *[other] Hosts and co-hosts may fill the whole list; anyone else may add up to { $count } songs. A song stays until it is removed or the room ends.
}

## Stage 2 of the room's song list: songs downloaded ahead.

# Said once on the room's status line where this browser ran out of
# storage while downloading the room's songs ahead: from then on only the
# likely next ones (the picked song, the one being looked at, the newest
# few) are downloaded ahead.
room-ahead-quota = This browser has no more room for the room's songs: only the likely next ones are downloaded ahead now.
# The status line on My songs for someone who chooses the room's songs:
# Enter there adds a song; picks are made on Room songs.
room-status-my-songs-picker = Add songs to the room here. Pick the next song on Room songs.
# The same for someone who may add songs but doesn't choose them.
room-status-my-songs-adder = Add songs to the room here.
# More's item on My songs for someone who may add songs: adds every song
# of the shown song's pack that is not in the room yet (as many as they
# may add).
# Variables:
#   $count (Number) - how many songs it adds
room-add-pack = { $count ->
    [one] Add this pack to the room (one song)
   *[other] Add this pack to the room ({ $count } songs)
}
# The status line on My songs while a pack's songs are being uploaded.
# Variables:
#   $done (Number) - songs uploaded
#   $count (Number) - the pack's songs being added
room-status-adding-pack = Adding the pack to the room: { $done } of { $count } uploaded.

## Round TW1 (room model revision 18): a host's Kick and who may join.

# The Players pop-up's toggle at its top right, for the host and the
# co-hosts: while it is on, each person they may remove has a Kick button
# on their row. It is off each time the pop-up opens (about 16 letters).
# Variables:
#   $on (Number) - 1 while it is on, 0 while it is off
room-moderate = { $on ->
    [1] Moderate: On
   *[other] Moderate: Off
}
# The button on a person's row while Moderate is on: removes them from
# the room at once (they may come back by the invite; about 8 letters).
room-kick = Kick
# The status line said to everyone when a host or co-host removed someone.
# Variables:
#   $name (String) - the person removed
#   $by (String) - who removed them
room-kicked-someone = { $by } removed { $name } from the room.
# The status line, and a notice's title, for the person a host or co-host
# removed from the room.
room-link-kicked = The host removed you from the room.
# The notice's text under that title: they may come back by the invite.
room-kicked-body = You can come back to this room by its invite link.
# The status line, a notice's title and an invite page's line for someone
# the room does not let in. One neutral sentence: never says why.
room-link-not-admitted = You can't join this room.
# The notice's text under that title.
room-not-admitted-body = Other rooms can still be joined by their invite links.
# Host options' row (round TW1): whose Twitch lists keep people out of the
# room; the room's owner's setting (about 20 letters).
room-host-twitch-bans = Twitch bans
# Its values, and the chat row's Off (about 14 letters each).
room-host-twitch-off = Off
room-host-twitch-bans-only = Bans
room-host-twitch-bans-blocks = Bans and blocks
# Host options' row: how much of the host's Twitch chat the room may show
# behind the play (each player still chooses for their own device).
room-host-twitch-chat = Show my Twitch chat
room-host-twitch-emotes = Emotes
room-host-twitch-messages = Emotes and messages
# Under those rows: whose Twitch account the setting uses.
# Variables:
#   $login (String) - that person's Twitch login
room-host-twitch-whose = { $login }'s Twitch bans and chat apply.
# Round TW1b: the status line after a trip through Twitch for the room's
# Twitch setting ended without connecting it (canceled at Twitch).
room-twitch-not-connected = Twitch access was not connected: nothing changed.
# Round TW3: the status line after a trip through Twitch for this
# device's "Twitch chat in rooms" (Options → Display) connected it.
room-twitch-chat-on = Twitch chat in rooms is on for this device.
# Desktop, the dialog of a trip through Twitch for the person's Twitch
# access (a room's Host options, Options' Twitch chat in rooms), while it
# starts and while the game reads what was allowed.
room-twitch-connecting = Connecting Twitch access…
# The same dialog while the browser is open on Twitch's page.
room-twitch-connect-waiting = Allow Twitch access in your browser, then come back here.
# Round TW3: the status line where the room's Twitch bans and chat turned
# off because this player's own Twitch access ended (Twitch refused it,
# or it was disconnected on another device). Nobody was removed.
room-twitch-ended = Twitch access ended: your bans and chat are off here.
# Round TW3: the status line, to everyone, where the person whose Twitch
# bans and chat the room used left the room, so they no longer apply.
# Variables:
#   $name (String) - that person's name
room-twitch-setter-left = { $name } left: their Twitch bans and chat are off.
