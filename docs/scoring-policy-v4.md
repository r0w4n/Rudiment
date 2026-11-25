# Scoring policy v4

Scoring policy `v4` is Rudiment's authoritative real-time policy. New attempts
store `v4`; saved v1-v3 sessions retain their stored results and versions and
are never recalculated. The v3 implementation remains callable only by the
checked-in feasibility studies so those historical comparisons stay
reproducible.

## Timing and event time

- Expected onsets use the active tempo map. Played timestamps are calibrated
  by subtracting the configured input latency.
- The configured inclusive matching window remains ±100 ms by default.
  Perfect (±20 ms), Good (±60 ms), Early/Late, note points (100/95/60), the
  50-point invalid-hit penalty, ratings and mastery rules are unchanged.
- Tone.Transport remains the musical clock. Its existing frame callback
  advances a scoring watermark 32 ms behind the transport position, with an
  additional allowance for positive input calibration. This is a
  delivery allowance for browser MIDI callbacks, not a second matching window.
- A note's valid hit window closes at its inclusive late boundary. It finalizes
  one additional matching window later (200 ms after onset by default), which
  gives the bounded local solver one-hop context for rapid repeated notes. An
  event timestamp older than an already advanced watermark is
  rejected and the attempt stops with a visible reliability error; a confirmed
  judgement is never rewritten to hide late delivery.
- Completion includes a silent 234 ms transport tail (200 ms decision horizon,
  32 ms delivery allowance and 2 ms boundary margin), extended by any
  positive input calibration. MIDI remains eligible
  during this tail, but the metronome and reference notes do not play.

Count-in time never advances playable scoring time. Pausing stops the transport
and therefore the watermark. Resume rebases absolute browser timestamps while
preserving the same musical times. A tempo change preserves elapsed history and
retimes only unconfirmed future targets.

## Local score-first assignment

Expected notes and played hits are ordered by musical time, with original note
index and arrival index as stable ties. Matching is one-to-one,
order-preserving, and separate per instrument. An edge exists only inside the
configured matching window.

When the earliest expected note reaches its fixed two-window decision frontier,
v4 constructs a local problem from:

1. unconfirmed expected notes reachable by hits delivered at that frontier;
2. unconsumed hits delivered at that frontier; and
3. the expiring expected note, even when it has no candidate.

The local ordered assignment is selected by these priorities:

1. maximize the final-score numerator within the local problem: each match is
   worth its existing judgement points plus 50 points for avoiding an invalid
   unmatched hit;
2. maximize matched pairs when the score contribution ties;
3. maximize exact raw MIDI-note matches;
4. minimize total absolute timing error, rounded to the nearest nanosecond for
   this tie only; and
5. choose the lexicographically earliest `(expected index, arrival index)` pair
   sequence.

Misses contribute zero note points. Both Extra and Wrong Instrument strikes use
the existing 50-point invalid penalty, so the objective exactly accounts for
the score calculation instead of maximizing match count as an unrelated proxy.

The candidate expected-note set is capped at that same two-window horizon, so
dense overlap cannot extend the decision chain. Only the expiring expected note
is confirmed from a local solve. A hit assigned to it is consumed; otherwise
the note is confirmed Missed. Assignments shown before their frontier are
provisional projections from the same local solver. They may change as
admissible hits arrive and cannot be confused with a confirmed Missed event.
Unassigned observed hits are likewise projected immediately as provisional
Extra or Wrong Instrument feedback, then disappear if a later admissible local
assignment uses them. They become confirmed invalid hits only after no future
expected onset can reach them.

This deliberately means a later hit outside a note's event-time horizon cannot
shift an entire dense passage backward to reopen that note. In the study's
ten-note counterexample, v4 keeps nine Perfect hits, one Missed note and one
Extra hit for a score of 85 instead of shifting the chain to obtain ten lower
quality matches and a score of 68.

## Identity, loops and lifecycle

Confirmed expected results include the expanded expected-note ID, instrument,
expected onset and judgement. Expanded IDs retain their `-loopN` suffix in the
engine; Leptos publishes the base ID plus explicit zero-based iteration to
Notation and Highway. Switching views therefore does not restart or duplicate
scoring.

Selected and noncontiguous bars are expanded onto the existing contiguous
practice timeline before engine construction. Repeat-until-Good finalizes the
attempt through the same engine, including its late drain, then decides from
confirmed results only. Each retry has a fresh engine; completed attempt
summaries are combined from their already-confirmed results rather than being
rematched. Restart and exercise navigation discard the active attempt.

At a loop transition the renderer projection is cleared, preserving the
existing no-stale-colours behaviour while the same scorer continues to retain
the authoritative results for every iteration. The visual projection keeps at
most the newest 256 unmatched-hit markers; older Extra/Wrong Instrument
results remain in the scorer and final summary. This bounds unreadable DOM
overlap during long continuous practice without dropping scoring events.

## Work limit and failure policy

The normal local matrix spans only the notes and hits reachable within the
matching horizon. A single local solve is capped at 16,384 expected×observed
cells. Exceeding the cap marks the scorer overloaded and stops the attempt with
an explicit error. Rudiment does not drop hits, publish guessed confirmations,
or switch to another matching policy. Expected, observed and confirmed result
storage is linear in the finite attempt; continuous practice retains its
existing bounded chunk lifecycle.

## Differences from v3

V3 rescored the complete session with a global, maximum-cardinality ordered
dynamic program, while live feedback used a separate nearest-hit greedy path.
Dense overlapping windows could allow a far-later hit to reassign much earlier
notes, so live and final results could disagree and a trustworthy live miss
could remain unresolved indefinitely.

V4 replaces both production paths with one incremental engine. It prioritizes
the actual score contribution inside a finite local horizon, has explicit
event-delivery and overload failures, emits confirmed live misses, and derives
the final summary from the same immutable confirmations. Assignment and score
changes relative to v3 are intentional policy-version differences; historical
records remain untouched.
