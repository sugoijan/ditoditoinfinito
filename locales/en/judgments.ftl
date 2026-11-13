# The DDI ruleset's judgment words (docs/plans/ddi-ruleset.md, "Timing
# tiers"), shown in the player's language when Options → Rules → Judgment
# words is "In your language" (the default): over the field during a play
# (one word, large, short-lived), in the results' rows ("Fantastic /
# Glorious"), a stage's Standings and Options' preview. Each tier has an
# early word and a late word. THE HINT: every late word has a sound the
# early words never have (in English a G: Amazing, Glorious, Great, Good;
# in Japanese a voicing mark ゛ in its katakana). Nothing tells the player
# this; keep it in your language if you can. Short: a word or two, read at
# a glance. Game words, not sentences: no final punctuation.

# A hit within the best window, early or late alike. Neither early nor late.
judgments-w0 = Absolute
# The second best, early.
judgments-w1-early = Awesome
# The second best, late (has the late hint).
judgments-w1-late = Amazing
# The third, early.
judgments-w2-early = Fantastic
# The third, late (has the late hint).
judgments-w2-late = Glorious
# The fourth, early.
judgments-w3-early = Nice
# The fourth, late (has the late hint).
judgments-w3-late = Great
# The fifth, early.
judgments-w4-early = OK
# The fifth, late (has the late hint).
judgments-w4-late = Good
# The worst hit, far too early: says it plainly.
judgments-w5-early = Too early
# The worst hit, far too late: says it plainly (English has no G here; a
# language that can keep its hint here too should).
judgments-w5-late = Too late
# A note that passed without being hit. Neither early nor late.
judgments-miss = Miss
# A press that hit no note and counted against the play (a stray press).
# Neither early nor late.
judgments-boo = Boo
# What the results call the count of stray presses, a row's label beside
# a number.
judgments-boos = Boos
