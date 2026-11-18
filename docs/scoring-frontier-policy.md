# Candidate incremental scoring policy (superseded study)

> Historical note: production scoring v4 now uses the bounded, score-first
> policy in [`scoring-policy-v4.md`](scoring-policy-v4.md). This document is
> retained to explain the rejected maximum-cardinality frontier prototype.

This is a proposal for a separately versioned future policy, **not** a change
to production scoring v3 or an approval to migrate saved results. The
experiment in `crates/core/examples/scoring_frontier.rs` is isolated. Any
production adoption requires a correctness and performance gate, explicit review
of this policy, and a new scoring version. Existing history must retain the
version and results with which it was recorded.

## Proposed rules

For each instrument, order expected notes by actual onset then original input
index, and played hits by calibrated timestamp then arrival index. Matches
must be order-preserving and one-to-one. Eligible edges use the existing
inclusive configured match window, latency calibration and tempo-map onsets.
Choose, in this order:

1. The greatest number of matches.
2. The greatest number of matching raw MIDI note numbers.
3. The smallest sum of absolute timing errors, each rounded to the nearest
   nanosecond for this optimization only.
4. The lexicographically earliest sequence of `(expected index, arrival index)`
   pairs. Arrival indices disambiguate equal timestamps; unmatched events do
   not appear in this sequence.

The nanosecond rounding makes accumulated optimization costs exact integers,
so prefix and suffix comparisons compose deterministically; the unrounded
error and existing inclusive classification thresholds, 100/95/60 points,
invalid-hit penalty, ratings and mastery rules are unchanged. Differences
smaller than a nanosecond per edge can nevertheless alter assignment, including
at a judgement boundary; those differences require explicit policy approval,
not an assertion that the policies are identical.

The raw-note preference and order preservation reflect v3, although exact-note
priority can choose a less precise hit and fixed ordering can prevent two
simultaneous notes from being cross-assigned. They need product review rather
than an implicit change. Equidistant/repeated/grace notes are resolved by the
same stable pair order. Separate instruments are optimized independently;
unmatched strikes retain v3's extra/wrong-instrument classification, including
the presence of nearby already-matched notes.

V3 instead resolves fully tied DP plans by traversal actions. An ineligible
extra hit can change which equal-quality pair is selected; this is a documented
consequence of the implementation, not an established musical preference. The
proposed stable-pair rule removes that dependence. It can change individual
note assignments, timing classifications, scores and awards on new runs even
when overall optimization objectives tie. Saved v3 results are not recalculated.

## Confirmation and limits

The candidate experiment treats the current optimum as provisional. To confirm
a prefix it considers optimal paths for each possible first future expected
note, provided the caller can promise that no later-delivered hit has an event
time before an irreversible watermark. A note is eligible for confirmation
only after its inclusive late boundary, and only if all relevant frontier
paths agree on its match or absence. An empty frontier or an unresolved
watermark is not evidence that a note was missed. The experiment is not an app
integration, and does not establish such a watermark for Web MIDI, pause,
seek, tempo edits, loop retries or the final transport boundary.

The proposed tie rule is a testable hypothesis, not a proof of musical
superiority. Dense simultaneous or adversarial inputs may still generate many
eligible edges, large frontiers, long traceback comparisons and unbounded
pending state. A work cap must fail explicitly without publishing unproven
confirmations; silently dropping hits or falling back to a different matcher
is not acceptable. Browser and hardware responsiveness must be measured
separately before production integration.
