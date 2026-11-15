# Incremental scoring study — Stage 1

**Decision: stop before production integration.** The isolated connected-window
prototype does not reproduce the current final scorer's tie-breaking and does
not bound work or confirmation delay for dense continuous passages. This is a
failure of this prototype, not proof that all unified incremental matchers are
unsuitable. Production matching, feedback and transport are unchanged.

## Existing paths and rules

- `web/src/main.rs::live_judgement_for_hit` greedily consumes the nearest
  unmatched expected hit of the same instrument within the configured match
  window. Equal distances favour the earlier beat. The handler plays the snare
  before scoring, then publishes live feedback to notation and the optional
  presentation host. Live feedback does not emit misses.
- `crates/core/src/lib.rs::score_session_with_tempo_map` computes an offline
  ordered dynamic program independently per instrument. It maximizes matches,
  then exact raw MIDI note matches, then minimizes absolute error. Equal plans
  prefer a match, otherwise the existing skip-expected/skip-observed order
  matters. It is an **order-preserving** matcher, not an unrestricted bipartite
  matcher; its exact-note tie-break cannot reorder simultaneous strikes.
- Both use `ScoringConfig` (defaults: Perfect ±20 ms, Good ±60 ms, matching
  ±100 ms, inclusive), calibrated observed time and the same piecewise tempo
  conversion. Points are 100/95/60 and invalid hits cost 50; ratings and mastery
  are calculated separately. The public nearest-neighbour-derived
  `matching_window_ms_with_tempo_map` is not the window used by these scoring
  paths. The final wrong-instrument check considers *all* nearby expected
  notes, whereas live considers only unmatched ones.
- An explicit reproduction: two snare targets at 0 and 80 ms, hits at 60 and
  150 ms. Live assigns the first hit to the 80 ms target and calls the second
  extra; final matches both (60→0, 150→80). A separate test demonstrates the
  wrong-instrument difference after a target has already been matched.
- Git history shows optimal scoring introduced in `1f4a8a2`; it does not
  establish why live remained separate. `PERFORMANCE.md` reports that live
  matching was already below the p95 target when profiled, and was not
  refactored; this is evidence for preserving responsiveness, not proof of the
  original design intent.

## Application constraints

Selected bars are expanded into a contiguous timeline and repeated expected IDs
are suffixed `-loopN`; presentation uses the base ID and explicit iteration.
Tempo changes retime the runtime map. Pause/resume rebases stored input
timestamps. Repeat-until-Good currently decides at the segment boundary from
live greedy results; failed attempts are added to the summary separately, with
synthetic missed IDs. Continuous loops are chunked. These behaviours all need
explicit migration and tests, not just a replacement matching function.

Tone.Transport provides the clock and the bridge calls application state on
its existing frame. The bridge stops and calls completion at the segment end;
a future engine must design an explicit late-window drain without a second
clock, including the final note and repeat boundaries. Web MIDI timestamps are
delivered through WebMidi on the main thread. The prototype assumes ordered
timestamps and an irreversible event-time watermark; the app has not established
that late delivery, pause, seek or tempo changes satisfy this assumption.
Notation and Highway have separate drawing paths fed by the current `live`
state. The Highway controller replaces its judgement map from that state;
notation also uses an append path. A provisional correction must update both,
retain iteration identity and remove stale visuals.

## Prototype and correctness

`crates/core/examples/scoring_study.rs` is an isolated experiment, not linked
into the application. It precomputes targets with the existing tempo map, forms
connected components of intersecting ±match windows, buffers ordered events,
calls the existing offline scorer when the watermark passes a component, and
assembles a summary from those results. For immediate *provisional* feedback
it invokes the same scorer on a local projection; this projection is not a
confirmed assignment and does not reliably distinguish simultaneous observed
events with identical timestamps. Its candidate confirmations are not safe to
publish: the experiment deliberately exposes a final-policy incompatibility.
Empty-gap extras, tempo/calibration, instrument identity and expanded loop IDs
are exercised. It does not implement transport pause/seek/restart, dynamic
tempo mutation, repeat attempts, an event-time delivery guarantee, or browser
integration. It buffers all expected notes and stores final output, so its total
memory is not constant even when pending groups are small.

The exhaustive test enumerates 880 ordered three-hit inputs at four note
spacings. **17 summaries differ** from the unmodified final scorer, including
assignments and, in some cases, result details. The specific counterexample has
targets at 0/50/100 ms and hits at -101/50/50 ms: the early extra is outside
every window, yet its presence changes the global DP's equal-plan traversal.
Scoring the remaining group alone assigns 50→0 and 50→50; full-session scoring
assigns 50→50 and 50→100. Dropping the extra makes the group and offline
results agree. Thus merely calling the existing scorer per component does not
preserve its deterministic tie-break. The exhaustive test records this known
failure intentionally; a passing test does not mean equivalence was achieved.

The prototype accepts a valid hit at the exact late boundary and can mark the
final note missed after its watermark; these are synthetic tests, not integrated
live-miss behaviour. Grace clusters and a synthetic expanded repetition, plus
a tempo-map/calibration/multiple-instrument case, are also compared offline.
They are not exhaustive coverage of real flams, drags, selected bars, seeking
or repeat-until-Good. In particular, future event timestamps cannot safely be
assumed absent based solely on a rendering frame without proving the delivery
contract.

## Measurements

Measured 2026-11-15 on macOS 25.6.0 arm64, 10 logical CPUs, Rust 1.99.1,
Node 22.23.1 and Chrome 157.0.8103.51. Native numbers are one release-mode run
per workload, not controlled latency distributions across repeated runs. Each
per-hit percentile comes from the stated number of hits. Targets are all snare,
the hit is 5 ms late, tempo is constant, and 60 BPM is used with beats chosen
to produce the requested spacing. Native live measurement calls the actual
private live function without the surrounding signal clones, audio or browser
bridge; native prototype hits include a local provisional score and any group
closures reached during the hit. The live test uses demo hits with no raw MIDI
note; the prototype uses MIDI note 38, so the exact-note objective is not the
same workload. The final group is measured separately.

| Notes / spacing | Live p50/p95/p99/max ms | Prototype p50/p95/p99/max ms | Largest group; pending hits | Prototype final close ms | Offline full score ms |
| --- | --- | --- | --- | ---: | ---: |
| 500 / 1000 ms | 0.0214 / 0.0314 / 0.0401 / 0.0500 | 0.0035 / 0.0047 / 0.0113 / 0.0869 | 1; 1 | 0.001 | 4.926 |
| 500 / 250 ms | 0.0149 / 0.0193 / 0.0278 / 0.0391 | 0.0020 / 0.0021 / 0.0035 / 0.0052 | 1; 1 | 0.001 | 3.254 |
| 500 / 125 ms | 0.0123 / 0.0177 / 0.0282 / 0.0546 | 0.0010 / 0.0012 / 0.0030 / 0.0067 | 500; 500 | 2.646 | 2.865 |
| 500 / 62.5 ms | 0.0115 / 0.0160 / 0.0227 / 0.0280 | 0.0007 / 0.0009 / 0.0037 / 0.0063 | 500; 500 | 3.777 | 3.616 |
| 1000 / 31.25 ms | 0.0178 / 0.0246 / 0.0309 / 0.0379 | 0.0019 / 0.0020 / 0.0037 / 0.0134 | 1000; 1000 | 14.637 | 9.645 |
| 2000 / 62.5 ms | 0.0222 / 0.0316 / 0.0435 / 0.0630 | 0.0007 / 0.0008 / 0.0009 / 0.0057 | 2000; 2000 | 35.350 | 23.302 |

The actual live function at 10,000 notes / 62.5 ms measured
0.1030/0.1449/0.1696/0.2317 ms p50/p95/p99/max. The prototype's cheap hit
percentiles **exclude its deferred final close** and are not an end-to-end
browser comparison. A 2,000-note group took 35.350 ms to close, or 46.520 ms
in a separate run; closure can instead occur on a later hit or transport frame.
The 2,000-note measurement's `time -l` process peak was 137,166,848 bytes,
including both prototype and offline score runs and runtime overhead; it is not
an isolated allocation count. Its 0.07 s elapsed / 0.06 s user / 0.01 s system
times are too short for meaningful CPU utilization conclusions. The underlying
DP allocates a (expected+1) × (observed+1) plan matrix, so a connected 10,000 ×
10,000 group would be impractical to assume safe without a new algorithm.
For 62.5 ms spacing the candidate intervals connect throughout the exercise:
this prototype confirms nothing until the chain closes, even though its local
provisional projection continues. Fewer confirmed misses and a deferred long
main-thread task fail the gate independently of hit-call percentiles.

The current app's existing full browser matrix (one repeat per fixture; 64,
480, 2,000, 10,000 and 2,560-note/simultaneous exercises) ran with simulated
MIDI. Aggregate callback p50/p95/p99/max was 0.20/0.70/0.80/1.80 ms (786
samples), MIDI-to-DOM mutation 9.70/11.95/12.23/12.30 ms (36), and audio
scheduling 0/0.10/0.20/0.70 ms (822). The 10,000-note interaction fixture's
callback p95 was 0.70 ms and score-update p95 2.70 ms. These are browser-level
measurements, not isolated Rust scorer timings; the harness's long-task and
heap observations also include import/notation and do not establish active
scoring memory. The full report is generated at the ignored
`performance-results/full.{json,md}`. The checked-in fast baseline is a
different workload and must not be used for a relative full-matrix claim.

Reproduce:

```sh
cargo test -p rudiment-core --locked --example scoring_study -- --nocapture
cargo test -p rudiment-web --locked --release scoring_study_live_matcher_benchmark -- --ignored --nocapture
cargo run -p rudiment-core --locked --release --example scoring_study
/usr/bin/time -l target/release/examples/scoring_study 2000 62.5 # macOS
npm run perf:full
```

Not measured: prototype in WASM/browser, allocation counts, sustained CPU,
physical MIDI/audio, iPad, long-session endurance or a systematic per-rudiment
performance matrix with ambiguous events, tempo changes and noncontiguous bars.
The existing browser matrix includes a noncontiguous selection exercise but is
not a per-hit correctness or latency comparison for the prototype. Nor does
headless simulation prove real-device latency or frame rates. These omissions
are additional reasons the production gate cannot pass.

## Validation

`npm run qa:full` passed all 13 check groups, including the production build,
Rust and JavaScript tests, dependency audits, browser tests, accessibility and
the fast performance gate. Within the browser suite, 97 tests passed and the
fullscreen test was skipped because the environment cannot grant fullscreen;
all 11 accessibility tests passed. The native benchmark is intentionally
ignored in normal unit runs and was run separately with the command above. An
initial `qa:quick` failed Rust formatting before the example was formatted; the
subsequent full gate passed. No endurance run or hosted CI result is claimed.

## Recommended next steps

Investigate an incremental ordered DP that preserves the *actual* global
tie-breaking, including skipped observations, and proves when all viable DP
frontiers agree on a prefix. A sparse/banded frontier and compact traceback may
avoid allocating a full quadratic matrix, but neither bounded memory nor
immediate confirmation follows automatically for arbitrarily dense, ambiguous
input. Establish an event-time watermark/delivery policy and lifecycle rules
before marking irreversible misses; decide how completion and bar retries allow
late hits without delaying audio. Test equivalence against the existing scorer
with exhaustive and randomized cases, raw-note ties and timestamp collisions,
then measure closure work and the full browser integration on representative
passages before reconsidering Stage 2. Any intentional tie-break change requires
an explicit scoring-version/product decision, not an unnoticed refactor.
