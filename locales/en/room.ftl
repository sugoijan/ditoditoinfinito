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

## The status line (over the list, or atop the room's panel): what the room waits for.

# The countdown to the stage's start.
# Variables:
#   $number (Number) - the stage, from 1
#   $seconds (Number) - whole seconds left
room-status-countdown = Stage { $number } starts in { $seconds }…

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

room-refused-preparing = The song is still being prepared.

## Why a stage's song could not be prepared on this device, on the
## status line.

room-load-unreadable = The song could not be read. Choose another.

room-load-changed = The chart changed since it was chosen. Choose it again.

room-load-cpu = The CPUs cannot play this chart. Choose another.

# An online room's song could not be downloaded from the room's share.
room-load-download = The room's song could not be downloaded.

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

## The Rules pop-up: this match's rules, locked while it runs, and the
## next match's, which can change.

room-rules-title = Rules

# The section of the rules the running match plays by.
room-rules-this-match = This match · locked

# The section of the rules the next match will play by.
room-rules-next-match = Next match

# Rows' labels (about 20 letters each).
room-rules-stages = Stages
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

# The Attacks row's value in an online room, where the rule cannot be
# turned on yet (about 32 letters).
room-rules-attacks-online = Off · not in online rooms yet

# How stage points are given: by place, a rule still under study
# (about 24 letters).
room-rules-points-value = By place · experimental

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
room-rules-level-note = A CPU's stars show its level: its strength relative to the chart, not a skill rating.
room-rules-points-note = Points for fields other than eight players follow an experimental rule.
# The same note where the room offers the Points row's choice: the choice
# matters only with nine players or more.
room-rules-points-bias-note = The Points choice changes nothing with eight players or fewer; fewer than eight follow an experimental rule.
room-rules-ruleset-note = Everyone plays the match under DDI with these rules, whatever their own options say.
room-rules-battle-note = The battle is provisional: your stage ends when your life runs out. With attacks, good timing fills a power bar, and a full bar sends eight shocks into another player's field.
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
#   $stages (Number) - the match's stages
room-rules-running = This match is on stage { $number } of { $stages } and keeps these rules:

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

# To the host of a room on a server that does not pass rooms on yet.
room-leave-host-stays = You are the host. The room stays open without a host, and you can come back by its link.

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

# Once the player has won the stage before the song's end (they finish
# the song): in the line under the opponents' small fields at the right.
room-play-won = Stage won

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

room-play-cue-final-stage = Final stage

# The player's life is about to run out.
room-play-cue-life-critical = Life critical

# The players still in the stage became this many.
# Variables:
#   $count (Number) - how many are left (4 or 2)
room-play-cue-survivors = { $count } players left

# The player's life ran out: out of this stage.
room-play-cue-eliminated = You're out

# The player and others went out last together: first place shared.
room-play-cue-stage-drawn = Stage drawn

# An opponent's full power bar was spent on an attack against the
# player: eight shocks are on their way into the player's field.
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

# Leader: whoever has the most life (short, one word).
room-attack-policy-leader = Leader

# A small muted line under the life gauge over the field (its right
# end), while the player's full bar waits: nobody can be attacked right
# now, or no safe moment in the song is near (about 20 letters). The
# player's policy stands by itself at the gauge's left end.
room-play-attack-waiting = Waiting for target

# On a stage's start prompt, where the match has attacks: the label of
# the row where the player chooses their attack policy, followed by
# ◀ Spread ▶ (about 12 letters).
room-targeting = Targeting

# One line beside or under that row: what the chosen policy does (up to
# about 55 letters). Spread.
room-attack-policy-spread-about = Your attacks go to whoever has been attacked least.

# Retaliate.
room-attack-policy-retaliate-about = Your attacks go to whoever attacked you last.

# Leader.
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
# stage, against how many CPUs, and the rule (out when life is gone).
# Variables:
#   $number (Number) - the stage, from 1
#   $stages (Number) - the match's stages
#   $cpus (Number) - CPU opponents
room-prompt-stage = Stage { $number } of { $stages } · { $cpus ->
    [one] against one CPU
   *[other] against { $cpus } CPUs
} on a taps-only copy · out when your life runs out

# The same for an online room's stage: the others play it on their own
# devices (not shown during the play).
# Variables:
#   $number (Number) - the stage, from 1
#   $stages (Number) - the match's stages
#   $others (Number) - the other players starting it
room-prompt-stage-online = Stage { $number } of { $stages } · { $others ->
    [one] against one other player
   *[other] against { $others } other players
}, each on their own device · out when your life runs out

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
# them turned off (they are on by default), and the Points choice when it
# is not the default Strong.
# Variables:
#   $ruleset (String) - the ruleset's name ("DDI")
#   $difficulty (String) - "easy", "normal" or "hard"
#   $boos (String) - "on" or "off"
#   $attacks (String) - "off" when the match's attacks are turned off, else "on"
#   $points (String) - "strong", "medium" or "mild"
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

# The same, when the player left the stage while playing (no points).
# Variables:
#   $number (Number) - the stage, from 1
#   $place (Number) - the place, from 1
#   $field (Number) - how many played the stage
room-mark-forfeit = Stage { $number }: left the stage · place { $place } of { $field } · no points

# After that note when the points came from the experimental rule for
# fewer than eight players (short).
room-mark-experimental = points experimental

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

# Desktop, while signing in: the browser is open on Twitch's page.
room-sign-in-waiting = Finish signing in with Twitch in your browser, then come back here.

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

# The status line after leaving.
room-link-left = You left this room.

# Under a person's name in the players' column (about 16 letters each).
room-role-host = Host
room-role-co-host = Co-host
room-role-admin = Admin
room-watching = Watching
room-next-match = Next match

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
# once the host lets them.
room-copy-watch-invite = Copy watch-only link
room-replace-invite = Replace invite link
room-sign-in-again = Sign in again

# The status line after More's invite items.
room-invite-copied = Invite link copied.
room-watch-invite-copied = Watch-only link copied.
room-invite-replaced = Invite replaced: the old link no longer works.
room-invite-not-replaced = Couldn't replace the invite.

# The status line for a while after joining a room whose player places
# were all taken: the player joined to watch. "Play" is the Play/Watch
# control's name (room-intent-play).
room-joined-to-watch = The room is full: you're watching. Choose Play when a place frees up.

# The status line when the host chooses the stage's song (shown to the
# others).
# Variables:
#   $number (Number) - the stage, from 1
room-status-host-chooses = The host chooses stage { $number }'s song.

## The match flow (M7b): song select is the room. The host picks each
## stage's song there; the others follow the host's song.

# The status line over the list (the host's) or at the top of the room's
# panel (the others'): one line.
room-status-pick-first = Pick a song to start the match.

room-status-host-picks-first = The host is choosing the first song.

# Variables:
#   $number (Number) - the stage, from 1
room-status-pick-next = Pick stage { $number }'s song.

# A stage's start screen gathers readiness: the stage starts once everyone
# is ready, or when the time is up (whoever is not ready then sits it out).
# Variables:
#   $number (Number) - the stage, from 1
#   $seconds (Number) - whole seconds left
room-status-gathering = Stage { $number } starts once everyone is ready · { $seconds } s

# For someone who was not ready in time, while the others play.
# Variables:
#   $number (Number) - the stage, from 1
room-status-sitting-out = You're sitting out stage { $number }; the others are playing it.

room-status-pick-new-match = Match over. Pick a song to start the next match.

room-status-stopped-pick = Match stopped. Pick a song to start the next match.

# The same where the next match has other rules than the stopped one (as
# after the Rules pop-up's Apply and restart).
room-status-stopped-new-rules = Match stopped. Pick a song to start the next match under the new rules.

room-status-host-picks-new-match = Match over. The host picks the next match's first song.

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
# Variables:
#   $percent (Number) - how much of the song is loaded, 0 to 100
room-state-loading-percent = Loading { $percent }%
room-state-load-failed = Load failed
room-state-sitting-out = Sitting out
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

# The heading over the players' match points in the players panel on the
# room's song select (about 8 letters, in small capitals' place).
room-panel-points = Points

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
#   $stages (Number) - the match's stages
room-picks-count = { $picked } of { $stages }

# Marks at the top and the bottom of a scrolled list: how many rows are
# hidden there (a tap scrolls).
# Variables:
#   $count (Number) - rows hidden
room-panel-more-above = ▲ { $count } more
# Variables:
#   $count (Number) - rows hidden
room-panel-more-below = ▼ { $count } more

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
#   $stages (Number) - the match's stages
#   $cpus (Number) - its CPU opponents
#   $rules (String) - its rules in a few words, as room-rules-words writes them ("DDI · Hard timing · Boos")
room-rules-locked-line = { $stages ->
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

# Under the song: the player was not ready in time.
room-prompt-sat-out = You weren't ready in time, so you sit this stage out.

# Under the song: the room canceled the stage's start (too few were ready,
# or the host picked again).
room-prompt-canceled = The stage's start was canceled.

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
#   $stages (Number) - the match's stages
room-table-title = Stage { $number } of { $stages }

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

# A row's life column for a player who left the stage (about 6 letters).
room-table-left = Left

# Three small columns at the table's end where the match has attacks,
# a number under each (their headings as the others': capitals small,
# very short: 4 to 6 letters). The attacks this player launched.
room-table-sent = Sent

# The attacks launched against this player.
room-table-taken = Taken

# The shocks (an attack's eight hazards) this player stepped on.
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
room-refused-watch-only = You came by a watch-only link: the host can let you play.

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

# In the line under the fields, for a few seconds: a player's life ran
# out.
# Variables:
#   $name (String) - who
room-watch-cue-out = { $name } is out

# The stage is decided: this player won it.
# Variables:
#   $name (String) - who
room-watch-cue-won = { $name } wins the stage

# One player attacks another (shocks are sent into the other's notes).
# Variables:
#   $from (String) - who attacks
#   $to (String) - who is attacked
room-watch-cue-attack = { $from } attacks { $to }

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
# The button before signing in.
room-invite-audience-sign-in = Sign in with Twitch to watch
# While the audience join is under way, and on the audience's page before
# the room is shown.
room-audience-joining = Joining the audience…

# Why a seat in the audience was refused or ended.
room-audience-full = The room's audience is full.
room-audience-closed = The host closed the audience.
room-audience-too-many-seats = You are already watching this room on two devices.
room-audience-link-unknown = This audience link isn't this room's, or it was replaced.
room-audience-none = This room has no audience to watch with.

# The audience's page: its title (window) and heading (header, small caps).
room-audience-title = Watching
room-audience-heading = Audience
# Its Back.
room-audience-leave = Stop watching
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
