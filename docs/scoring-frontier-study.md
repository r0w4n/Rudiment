# Incremental scoring study — frontier prototype

> Historical note: this rejected prototype remains reproducible, but the
> production score-first implementation is documented in
> [`scoring-policy-v4.md`](scoring-policy-v4.md).

**Decision: the production gate does not pass.** The experiment improves on
the connected-window prototype for ordinary dense input, but it does not meet
the requested missed-note, bounded-work, lifecycle and browser integration
requirements. Production v3 scoring, history, notation and Highway are not
changed. This is a result for the tested algorithm and proposed policy, not a
proof that all unified scorers are impossible.

## Policy and musical implications

The candidate policy and its proposed version boundary are described in
[`scoring-frontier-policy.md`](scoring-frontier-policy.md). It retains v3's
ordered one-to-one matching, match-count and raw-note priorities, windows,
classifications and points, but replaces traversal-dependent ties with stable
pair ordering and ranks errors rounded to nanoseconds. It is an experiment,
not an approved production policy. Saved v3 sessions are not migrated.

On the original 880 ordered small cases, the candidate agreed with an
independent exhaustive oracle in every case; **23 cases had different
expected-note/timestamp assignments from v3**, and none of those 880 had a
different aggregate score. The extra-hit counterexample changes solely because
the proposed tie rule excludes unmatched events from its pair sequence.
Another explicit test demonstrates that nanosecond rounding can change a
Perfect/Good boundary assignment and score, even though the timing difference
is extremely small: the candidate scores 47.5 and v3 scores 50.0 on that
two-note boundary example. The 880 cases do not bound historical impact.

The primary objective also deserves product review, independently of the
historical tie-break. In a tested ten-note, 62.5 ms-spaced passage with one
miss and a later extra, v3 and this proposal maximize matches by shifting eight
hits late: **10 matched, score 68**. A plausible local interpretation gives
nine Perfect hits, one miss and one extra, yielding **85** under the same point
weights and penalty. This is a policy choice, not a demonstrated bug in the
current stated objective. One possible alternative is to maximize the
unrounded session points (equivalently, maximize each matched note's points
plus the avoided 50-point extra penalty), then apply explicit deterministic
ties. That would materially change which hits match, even though the weights
are unchanged. It has not been approved, implemented or benchmarked here, and
it does not automatically guarantee bounded confirmation for dense ties.

## Algorithm and correctness

`crates/core/examples/frontier/engine.rs` maintains a sparse ordered dynamic
program per instrument. For each arriving hit, binary searches narrow the
eligible expected notes; a Fenwick prefix structure chooses the best path
ending before each candidate. Updates are deferred until all candidates for
the same hit are evaluated, enforcing one-to-one matching. A persistent arena
records traceback. A current best-path projection is provisional and may
change; the prototype emits revised projections when its current path changes.
Unmatched hits and expected-note confirmations use the core's existing
classification, point and wrong-instrument rules. Final aggregate statistics
are derived from its confirmations, not a second final matcher.

For a **guaranteed monotonic calibrated event-time watermark**, an expected
note whose inclusive late boundary has passed is considered for confirmation.
For each possible first future expected note, the prototype queries the best
past prefix and confirms only the common note assignments across all those
frontiers. The integer cost and ordered pair tie-break make the proposed
prefix objective composable. The implementation is conservative: it does not
prove all early confirmations that might be possible. It does not claim that
testing establishes a formal proof for all floating-point inputs.

The independent offline oracle enumerates ordered matchings rather than using
the Fenwick implementation. The tests compare confirmed events during input,
final note and extra identities, classifications, error statistics and score
against that oracle for the original 880 cases and 1,000 seeded multi-instrument
small sequences. Tests also cover raw-note preference, simultaneous timestamps,
provisional reassignment, a grace cluster, tempo/calibration, loop-suffixed
IDs, strict inclusive boundaries, and the original v3 tie counterexample. These
are useful checks, not exhaustive proof. Pause, seek, runtime tempo changes,
repeat attempts and real application lifecycles are **not implemented** by
the experiment.

### Why a miss can remain unresolved

The test `one_later_hit_can_change_an_already_closed_note_under_the_primary_objective`
uses expected onsets 0, 62.5, 125 and 187.5 ms and played hits 5, 130 and
192.5 ms. At 192.5 ms the second note's ±100 ms late window is closed. Without
another hit it is missed. A further hit at 193.5 ms permits all four notes to
be matched by shifting the existing hits, which wins on cardinality. Both
candidate-policy outcomes are checked by the independent oracle. The same
construction extends through arbitrarily many overlapping notes; confirming the
miss before the extra is impossible under these stated rules, not simply a
failure to notice a closed window.

A 64-note regression case with misses at indices 3, 10 and 28 confirms the
first two but leaves index 28 and subsequent decisions unresolved; a later
extra changes the optimal result for index 28. This is retained as a negative
test, not excluded or converted into a claimed success.

The caller's watermark is another independent blocker. If a frame advances it
past 100 ms and a queued MIDI event with timestamp 100 ms arrives afterwards,
the prototype reports `LateDelivery`, but it has already emitted a miss that
the offline result would match. An error after the fact cannot make that
confirmation safe. WebMidi forwards browser event timestamps; the app has not
established a delivery watermark or late-event bound. Tone's final callback
also stops at the exercise/attempt boundary, before some late windows close.

## Work and memory limits

The prototype has an explicit per-operation work counter and a one-million-node
per-instrument cap. Exhaustion returns `Overload` and invalidates the attempt;
it does not silently drop input or switch matchers. This is an **explicit abort,
not a viable recovery strategy** for practice or performance. A failing call can
partially mutate the prototype's public event buffer; its caller must discard
that call and invalidate the attempt. The API does not enforce this isolation.
The budget counts
abstract operations, not wall-clock milliseconds. Initialization, allocation,
final summary sorting and platform scheduling are not guaranteed real-time or
covered by that work counter. No proof of a constant confirmation delay or
constant memory exists.

Expected notes, played hits, output and arena nodes are retained for the whole
attempt. Stable prefixes are not reclaimed. Each path tie and projection can
walk or allocate in proportion to the unresolved history. In the 10,000-note
gap run, the unresolved queue grew to 7,500 hits; the 500-simultaneous-note
run aborted on the **15th input** with the configured 100,000-unit work cap.
Neither behavior is acceptable for production.

## Native measurements

Measured on macOS 25.6.0 arm64, Rust 1.99.1, 2026-11-15. These are single
release-mode runs; hit percentiles have the number of hit samples shown by the
workload, not independent repeated trials. Synthetic snare hits are 5 ms late,
with demo input and no raw note. Native current-live numbers call the actual
private live function without signal cloning, audio or the browser bridge.
The candidate includes its projections and confirmations; its native allocator
instrumentation adds atomic overhead. Offline timings are full v3 scores, not
per-hit costs. Do not equate any of these with browser responsiveness.

| Workload | Current live p50/p95/p99/max ms | Frontier p50/p95/p99/max ms | Confirmed before finish | v3 final ms |
| --- | --- | --- | ---: | ---: |
| 500 notes, 1000 ms spacing | .0164 / .0203 / .0279 / .0489 | .0018 / .0022 / .0079 / .0237 | 499 | 5.70 |
| 500, 250 ms | .0130 / .0172 / .0264 / .0565 | .0010 / .0014 / .0039 / .0059 | 499 | 3.53 |
| 500, 125 ms | .0103 / .0148 / .0206 / .0256 | .0009 / .0011 / .0020 / .0035 | 499 | 3.13 |
| 500, 62.5 ms | .0092 / .0129 / .0174 / .0213 | .0013 / .0015 / .0033 / .0100 | 498 | 2.77 |
| 1000, 31.25 ms | .0159 / .0210 / .0270 / .0379 | .0020 / .0025 / .0060 / .0282 | 996 | 16.69 |
| 2000, 62.5 ms | .0225 / .0313 / .0393 / .0564 | .0010 / .0011 / .0023 / .0149 | 1998 | 39.41 |
| 2000, 62.5 ms, one gap | .0213 / .0257 / .0310 / .0499 | .0199 / .0569 / .0638 / .0712 | 500 | 23.07 |
| 10000, 62.5 ms, one gap | .1036 / .1370 / .1617 / .2255 | .1052 / .3110 / .3766 / .4812 | 2500 | not run |

The two gap workloads use the same synthetic hit timestamps for both native
matchers. Their p95 increases are respectively **0.0312 ms / 121%** and
**0.1740 ms / 127%** in these single runs; the native functions perform
different amounts of work and this is not an end-to-end app comparison. The
10,000-note frontier finish including aggregate summary was 0.52 ms in that
timed run, but it had not confirmed the gap during playback. Its `time -l`
process peak was 10,502,144 bytes, not an isolated allocator measurement.
That synthetic burst used about 1.28 s of user CPU over 1.28 s elapsed, roughly
one logical core while it ran; this is not sustained real-time utilization.

The same native runner also parses tracked MIDI fixtures and plays every
expected onset 5 ms late with its exact raw MIDI note. In one run: beginner
single strokes (8 notes) had candidate p95/max 0.0012/0.0012 ms; single
paradiddle (64) 0.0004/0.0009 ms; hard flams (64, including 32 grace notes)
0.0006/0.0009 ms; flam paradiddle (80, 16 grace) 0.0005/0.0014 ms; and
single drag (96, 64 grace) 0.0005/0.0005 ms. Their candidate and v3 scores
were all 100; these are short, clean performances, not ambiguous or real-device
measurements, and do not establish tail latency with adequate samples.

With a simulated 60 Hz watermark, a simple 500-note/250 ms run confirmed notes
at most 116.7 ms after onset. In 500-note and 2,000-note continuous 62.5 ms
gap runs, the gap remained unconfirmed during the exercise and was first
confirmed after the final window, respectively **23.49 s and 93.8 s after its
onset**. The frame-work p95 values were approximately 0.0015 ms and 0.0076 ms
native. The frame schedule and event-time watermark are synthetic; they do not
establish a browser delivery guarantee.

Instrumented allocation counts cover successful allocation/reallocation
requests during hit processing and final summary, including benchmark-owned
sample buffers; bytes are cumulative requested bytes, not retained memory.
At 2,000 dense hits without a gap: 16,003 allocations, 55 reallocations and
2,125,374 bytes requested. With a gap: 24,712 allocations, 73 reallocations and
100,880,298 bytes requested. At 10,000 with a gap: 140,668 allocations, 95
reallocations and 2,427,703,562 bytes requested; this is allocation churn, not
2.4 GB of retained memory. A 100,000-note no-gap synthetic native run retained
299,998 arena nodes and had a 67,223,552-byte process peak, with 122,732,878
cumulative requested bytes. It ran as a burst, not a real-time endurance session.
Allocation instrumentation changes the measured timings.

## Browser and application baseline

The isolated Chrome 157/WASM harness ran three repetitions per workload on the
same macOS/arm64 machine. Per-hit browser measurements include the JS/WASM
call but are quantized by `performance.now()` at approximately 0.1 ms; a
reported zero is below resolution, not zero work. They do **not** enter the
production MIDI handler or run Tone alongside the prototype.

| Workload | WASM per-hit p50/p95/p99/max ms | Result before finish | Linear memory high-water |
| --- | --- | --- | ---: |
| 2000, 62.5 ms, no gap | 0 / 0 / 0.1 / 0.1 | 1998 confirmed | 2,162,688 bytes |
| 2000, 62.5 ms, gap | 0 / 0.1 / 0.2 / 0.2 | 500 confirmed, 1500 pending, no miss | 2,162,688 bytes |
| 10000, 62.5 ms, gap | 0.1 / 0.4 / 0.5 / 0.6 | 2500 confirmed, 7500 pending, no miss | 7,471,104 bytes |
| 500 simultaneous notes | 0.2 / 0.3 / 0.3 / 0.3 | explicit overload on input 15 | 1,507,328 bytes |

In a separate browser run with a simulated 60 Hz watermark, the 500-note gap
was not confirmed during the exercise; the synthetic drain confirmed it
23.49 s after onset. The script processes up to 256 simulated hits per JS task
and yields between batches. Its worst 10,000-gap batch was 108.7 ms and the
Long Tasks API reported 45 tasks, maximum 108 ms. This is a deliberately
batched stress workload, **not** a measurement of individual real MIDI events,
the production app's dropped frames or acoustic latency.

The existing full Chrome app matrix was also rerun separately: callback
p50/p95/p99/max 0.20/0.70/0.80/1.70 ms (786 samples), and audio scheduling
0/0.10/0.20/0.80 ms (822). Those app workloads include simulated MIDI, audio,
rendering and different event timing; they are not equivalent to the isolated
WASM workload and cannot establish the candidate's effect on MIDI or audio.
Raw local artifacts are ignored under `performance-results/`.

A separate 30-second existing-app endurance smoke (not this prototype)
captured 23 heap samples ranging from 18.1 MB to 40.8 MB; after Restart it
reported zero markers, active voices and scheduled voices, and no active run.
Chromium heap sampling without controlled GC does not prove a leak or its
absence. This smoke is not a 15-minute endurance run or validation of the
candidate's long-session behavior.

## Reproduction

```sh
cargo test -p rudiment-core --locked --example scoring_frontier -- --nocapture
cargo test -p rudiment-web --locked --release scoring_study_live_matcher_benchmark -- --ignored --nocapture
cargo run -p rudiment-core --locked --release --example scoring_frontier
/usr/bin/time -l target/release/examples/scoring_frontier 10000 62.5 gap # macOS
cargo rustc -p rudiment-core --locked --example scoring_frontier --release --target wasm32-unknown-unknown -- -C link-arg=--export=frontier_setup -C link-arg=--export=frontier_hit -C link-arg=--export=frontier_advance -C link-arg=--export=frontier_finish -C link-arg=--export=frontier_confirmed -C link-arg=--export=frontier_nodes -C link-arg=--export=frontier_pending -C link-arg=--export=frontier_misses
node scripts/performance/scoring-frontier.mjs
npm run perf:full
node scripts/performance/run.mjs --tier endurance --duration-ms 30000 --output performance-results/frontier-endurance-smoke.json
```

The WASM script is a diagnostic spike, not a new production timer or MIDI
path. It writes ignored JSON and uses the existing Playwright/Chrome tooling.
Physical iPad, electronic drums, real audio and USB latency were not tested.
No 15-minute real-time endurance run, representative per-rudiment browser
matrix, or integrated candidate MIDI/audio benchmark was performed.

## Decision and next steps

Do not integrate this candidate. Its ordinary dense prefix behavior is
encouraging and its tested results match the proposed oracle, but: (1) the
primary policy permits far-future hits to change earlier misses, (2) real
event-time watermarks and lifecycle transitions are unproven, (3) the
simultaneous-hit cap aborts rather than providing a usable recovery, (4) long
unresolved chains cause substantial repeated traversal and allocation, and
(5) the prototype has not been integrated into the actual MIDI/audio path.
The candidate also adds enough policy and lifecycle complexity that the
maintainability gate has not been demonstrated.

Before another production attempt, obtain a product decision on whether
global maximum cardinality and raw-note priority outweigh local musical
plausibility and timely misses. Evaluate score-first or another explicit
versioned policy with counterexamples, not a silent heuristic. Independently
develop a proven delivery/watermark and end-of-attempt drain, compact frontier
state with a safe overload recovery, and measure the actual integrated browser
path. Any policy change leaves saved v3 history untouched.
