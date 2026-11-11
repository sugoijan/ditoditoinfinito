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

# A short line under the players' heading: what kind of room this is.
room-practice-note = Prototype · this device only

# The heading over the players' column (about 16 letters).
room-players = Players

# Beside the players' heading: how many play, of how many may.
# Variables:
#   $players (Number) - players in the room, CPUs included
#   $cap (Number) - the most the room allows
room-players-count = { $players } / { $cap }

# On a phone, the players in one line under the header.
# Variables:
#   $cpus (Number) - how many CPU opponents
#   $level (Number) - their level, from 1
#   $levels (Number) - how many levels there are
room-players-line = { $cpus ->
    [one] You and one CPU · level { $level } of { $levels }
   *[other] You and { $cpus } CPUs · level { $level } of { $levels }
}

# The local player's name in the players' column (about 16 letters).
room-you = You

# A CPU opponent's name in the players' column.
# Variables:
#   $number (Number) - which CPU, from 1
room-cpu-name = CPU { $number }

# Under a CPU's name: its level, the CPU's strength relative to the
# chart (not a skill rating).
# Variables:
#   $level (Number) - its level, from 1
#   $levels (Number) - how many levels there are
room-cpu-level = Level { $level } of { $levels }

# At the right of a CPU's row before the match starts: it takes its seat
# when the match starts (about 10 letters).
room-cpu-at-start = At start

# At the right of the player's row (about 10 letters each).
room-state-ready = Ready
room-state-not-ready = Not ready
# During a stage.
room-state-playing = Playing

# At the right of a row during a match: the points so far.
# Variables:
#   $points (String) - the points, as the room keeps them ("4", "7/2")
room-points = { $points } pts

# A stage's number on its row in the stage list (about 10 letters).
# Variables:
#   $number (Number) - the stage, from 1
room-stage = Stage { $number }

# Under a stage's song on its row: the stage and its chart.
# Variables:
#   $number (Number) - the stage, from 1
#   $chart (String) - the chart's difficulty and meter ("Hard 9")
room-stage-chart = Stage { $number } · { $chart }

# A stage's row with no song yet.
room-stage-empty = No song yet

# Under the song when the stage has none (about 30 letters).
room-no-song = Choose a song for this stage.

# The button under the song: opens song select to choose the stage's
# song (about 16 letters).
room-choose-song = Choose song

# The same button when the stage has a song.
room-change-song = Change song

# Under the song of a stage that has started: it cannot change.
room-stage-locked = This stage has started; its song stays.

# The header's buttons.
room-rules = Rules

# The bottom right's buttons (about 16 letters each).
room-ready = Ready
room-unready = Cancel ready
room-start-match = Start match
# Variables:
#   $number (Number) - the stage, from 1
room-start-stage = Start stage { $number }
room-cancel-start = Cancel start
room-stop-match = Stop match
room-new-match = Next match

## The status line at the bottom left: what the room waits for.

# Variables:
#   $number (Number) - the stage, from 1
room-status-choose = Choose a song for stage { $number }.

room-status-not-ready = Press Ready when you are, then Start match.

room-status-ready = Ready. Start when you like.

# A readiness window: who is not ready by then sits the stage out.
# Variables:
#   $seconds (Number) - seconds left
room-status-window = Starting in { $seconds } s; anyone not ready sits this stage out.

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

room-status-stopped = Match stopped. Finished stages keep their results.

room-status-over = Match over.

## Why something was refused, on the status line.

room-refused-no-song = Choose a song for this stage first.

room-refused-not-ready = Press Ready first.

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

# Its name in its header, in small capitals: written in capitals where
# the language has them.
# Variables:
#   $number (Number) - the stage, from 1
room-pick-heading = STAGE { $number } SONG

# The Enter key's hint: chooses the chart shown for the stage (a word or
# three).
# Variables:
#   $number (Number) - the stage, from 1
room-pick-use = Use for stage { $number }

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
room-rules-level = CPU level
room-rules-battle = Battle
room-rules-points = Points

# The CPU level's value.
# Variables:
#   $level (Number) - the level, from 1
#   $levels (Number) - how many levels there are
room-rules-level-value = { $level } of { $levels }

# The battle rules' value: not decided yet.
room-rules-battle-value = Survival · provisional

# How stage points are given: by place, a rule still under study
# (about 24 letters).
room-rules-points-value = By place · experimental

# Lines under the rows, muted.
room-rules-level-note = A CPU's level is its strength relative to the chart, not a skill rating.
room-rules-points-note = Points for fields other than eight players follow an experimental rule.
room-rules-locked-note = Changes apply to the next match; this one keeps its rules.
room-rules-clears-note = Applying clears readiness.

# The buttons that lower and raise a number (one sign each).
room-rules-less = −
room-rules-more = +

# The button that saves the changes.
room-rules-apply = Apply

## The More pop-up.

# Leaves the room for the multiplayer home, forgetting the practice.
room-leave = Leave room

## A stage's play: a quiet column of the CPU opponents beside the
## field, each with its name, level, life bar and Out once its life is
## gone (a narrow column: a few letters each).

# A CPU's level, short.
# Variables:
#   $level (Number) - its level, from 1
room-play-level = Lv { $level }

# A CPU whose life is gone: out of this stage.
room-play-out = Out

# Once the player has won the stage before the song's end (they finish
# the song): in the line under the opponents' small fields at the right,
# or over the column.
room-play-won = Stage won

# Under the quiet column of opponents beside the play (where there is no
# room for their small fields), when there are more opponents than it
# holds: how many are not shown. Short.
# Variables:
#   $count (Number) - how many are not shown
room-play-more = +{ $count } more

## A battle's moment no announcer line said, for a few seconds: in the
## line under the opponents' small fields at the right, or at the
## column's top (as narrow: about 22 letters).

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

# A note on a stage's results and under the song in the room: how the
# player did, by how the stage ended for them.
# Variables:
#   $number (Number) - the stage, from 1
#   $place (Number) - the place, from 1 (tied players share it)
#   $field (Number) - how many played the stage
#   $points (String) - the points it gave, as the room keeps them ("3", "5/2")
room-mark-survived = Stage { $number }: place { $place } of { $field } · +{ $points } pts

# The same, when the player's life ran out during the stage.
# Variables:
#   $number (Number) - the stage, from 1
#   $place (Number) - the place, from 1 (tied players share it)
#   $field (Number) - how many played the stage
#   $points (String) - the points it gave, as the room keeps them ("3", "5/2")
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
#   $points (String) - their points, as the room keeps them
room-rank-points = #{ $rank } · { $points } pts

# The status line between stages, when not ready.
# Variables:
#   $number (Number) - the next stage, from 1
room-status-next = Press Ready when you are, then Start stage { $number }.

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
room-hint-select = Select
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

# A short line under the players' heading in an online room.
room-online-note = Online · prototype

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
room-replace-invite = Replace invite link
room-sign-in-again = Sign in again

# The status line after More's invite items.
room-invite-copied = Invite link copied.
room-invite-replaced = Invite replaced: the old link no longer works.
room-invite-not-replaced = Couldn't replace the invite.

# The status line when the host chooses the stage's song (shown to the
# others).
# Variables:
#   $number (Number) - the stage, from 1
room-status-host-chooses = The host chooses stage { $number }'s song.

# Under the song card when the stage has none yet, for someone who does
# not choose it.
room-no-song-yet = No song chosen yet.
