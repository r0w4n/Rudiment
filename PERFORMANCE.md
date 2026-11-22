# Rudiment performance engineering

Rudiment profiles its production WASM/Leptos, VexFlow, Tone.js and Web MIDI path in real Chrome. The harness is not a substitute scoring engine: fixtures enter through the same MIDI import callback, simulated device events enter through Web MIDI, keyboard events use the window listener, and feedback is observed in the rendered SVG.

## Runtime map

MIDI files are fetched/file-read in `web/js/bridge.js`, parsed and quantised by `crates/core`, stored in Leptos state, then engraved by VexFlow. During practice, Tone.Transport schedules metronome/reference audio independently of `requestAnimationFrame`. Web MIDI or keyboard events cross into the Leptos hit handler, the authoritative Rust scorer updates its provisional/confirmed projection, and the bridge paints the changed note or unmatched-strike marker in the input turn. The existing transport frame advances event time for confirmed misses and the slower UI tick updates counters and session state. Completion drains that same scorer and persists its summary to IndexedDB.

| Concern                                                                        | Implementation                                            |
| ------------------------------------------------------------------------------ | --------------------------------------------------------- |
| MIDI parsing, quantisation, tempo map, scoring                                 | `crates/core/src/lib.rs`                                  |
| App state, repetition, transitions, keyboard input, persistence calls          | `web/src/main.rs`                                         |
| Web MIDI, audio scheduling, notation, playhead, scrolling, feedback, IndexedDB | `web/js/bridge.js`                                        |
| Optional Highway presentation and SVG drawing                                  | `web/js/highway-controller.mjs`, `web/js/highway-svg.mjs` |
| Real-browser workloads and reports                                             | `scripts/performance`, `tests/performance`                |

## Commands and tiers

```bash
npm run perf:fast       # Tier A: 3 repetitions of small/medium plus interaction traffic
npm run perf:full       # Tier B: full matrix through 10,000 notes and dense simultaneous hits
npm run perf:endurance  # Tier C: 15-minute continuous loop, lifecycle and heap sampling
npm run perf:report     # render JSON results as Markdown
npm run perf:compare    # compare current fast results with the checked-in baseline
npm run perf:trace      # full suite with Playwright trace ZIPs
```

For an abbreviated endurance smoke test, run `node scripts/performance/run.mjs --tier endurance --duration-ms 30000 --output performance-results/endurance-smoke.json` after `npm run build`.

Tier A is suitable for pull requests. Tier B is on-demand and release-facing. Tier C is scheduled/on-demand and never requires MIDI hardware. Reports and traces are written below ignored `performance-results/`; baseline JSON and the before/after report are versioned.

## Workloads and observations

`scripts/generate-fixtures.mjs` deterministically creates 60, 120, 180 and 240 BPM SMFs: 64-note small, 480-note medium, 2,000-note large, 10,000-note extreme, and a dense 240 BPM exercise containing eighth-subdivision bursts and simultaneous kick/snare hits. The full suite combines those imports with rapid MIDI/keyboard traffic, feedback painting, playhead frames, scrolling, modal use, and pause/resume/restart transitions.

Interaction workloads also select bars 2, 3 and 5, start two repetitions and wait for the jump to bar 5. The recorded `noncontiguous.jumpResponseMs` is a diagnostic musical-duration observation, not a latency budget; it is captured after the existing gated measurements and does not change their baseline workload.

After those existing measurements the harness selects Highway and records the actual SVG renderer's per-update work separately as `highway.renderer_update_ms`. This diagnostic is not a notation or controller-only measurement and is not yet a gated baseline. The dense kick/snare fixture exercises the same multi-lane Highway used by imported kit exercises.

Directly observed values include callback work, import-to-render, notation rendering, score/feedback work, input-to-DOM-mutation latency, animation intervals, Long Tasks, response times, and Chromium precise heap size. Frame drops are estimated from intervals above 16.7 ms. `AudioContext.getOutputTimestamp()` helps align scheduled visual pulses, but the harness does not measure acoustic output, USB/driver latency, compositor-only work, or GC pauses unavailable through stable browser APIs.

## Findings and optimisations

The baseline identified notation layout as the dominant long task. The 10,000-note fixture repeatedly queried the complete SVG once for every one of 625 notation systems while calculating inter-system mark clearance. A single indexed DOM traversal removed that quadratic pass without changing engraving. Live input matching was already below the 2 ms p95 target and was not refactored.

Input processing was fast but note colour feedback waited for a 32 ms application-state tick. Feedback is now coalesced onto the next animation frame; UI counters remain throttled and Tone.Transport remains independent of rendering.

A Chrome regression test also exposed backward playhead jumps during hits: the feedback frame reused the slower UI progress value. Feedback now paints without repositioning the playhead, leaving position and scrolling to the transport-driven frame. The test observes SVG position mutations during repeated keyboard hits; it does not measure subjective smoothness or physical MIDI latency.

Further playhead investigation found that Tone's `Transport.ticks` uses the
clock's rounded tick at Tone's scheduling `now()` (100 ms ahead of the raw
audio context). The metronome sample is scheduled for its actual output time,
so that future position gave both playheads a systematic lead even with
otherwise smooth frames. Visual frames and pause/resume now query the _same_
Tone transport with fractional `getTicksAtTime()` at the estimated audio
output time (`getOutputTimestamp()` where available and valid, raw
`AudioContext.currentTime` otherwise). This is a position read, not a new
clock, extrapolator or audio scheduler. Frames delayed by scoring or browser
work render the current transport beat on the next opportunity.

An iPadOS field report (2022 iPad Air, 60 Hz) described both the notation
playhead and the opt-in wall-clock reference overlay lurching together, with
no MIDI hits and even at 30 BPM. Lock-step motion of both rAF loops rules out
the audio-clock bridge and feedback painting; it means frames themselves
stall. Auditing periodic work found exactly one main-thread activity during
otherwise steady practice: the practice-duration checkpoint wrote an
IndexedDB readwrite transaction every 5 seconds (`bridge.js`
`practiceTime`). That cadence is now 30 seconds; `stop()` on pause,
navigation, visibility change and pagehide still persists immediately and
`loadPracticeTime()` forces a checkpoint on read, so only a hard crash can
lose up to 30 seconds of a non-scored duration counter. A Playwright WebKit
probe (macOS, headless and headed, using the `?playhead-debug=1`
instrumentation) measured p50 17 ms frame intervals with no stalls over
32-second runs both before and after the change, so desktop WebKit does not
reproduce the report and the improvement must be confirmed on the device.
A low-cadence hidden-SVG playhead mirror was also tried and reverted: the
performance-correctness suite samples `#playhead` `data-beat` per frame as
the notation timeline readout (0.12-beat budget at 240 BPM), which a 100 ms
mirror cadence violates.

The optional Highway previously allocated/froze note and grid snapshots and
walked the whole visible SVG on every frame. It now retains guarded static
beat-window projections, updating only the moving SVG group between window
or feedback changes. Notation keeps its existing VexFlow engraving path,
caches the playhead element and viewport height, and avoids a synchronous
layout read during normal per-frame scrolling; metronome labels and beat LEDs
update only when their displayed beat changes. No full-score redraw was
needed for position or feedback. The performance harness continues measuring
the actual SVG renderer separately from notation.

On macOS/ARM Chrome 157 the fast medium-fixture interaction workload passed
its absolute gates. The final run measured 0.30 ms p95 for the actual highway
renderer (208 updates), 1.69 ms p95 for notation score updates (42 updates)
and 78.78 ms p95 for animation-frame intervals (34 samples). The initial
unmodified run measured 0.30 ms, 1.69 ms and 71.10 ms respectively; these
small, burst-heavy samples do **not** demonstrate a frame-rate improvement. The 250
synchronous synthetic MIDI hits still block the main thread and cannot be
painted during the stall. The full run measured highway renderer p95 of 0.30
ms (large), 0.20 ms (extreme) and 0.60 ms (dense), and 78.74 ms animation
interval p95 across its interaction workloads. A dense 2,560-hit/240 BPM
browser regression checks both views during frequent hits and a deliberate
90 ms stall; it verifies that the next frame recovers the current transport
position. A separate 30-second medium-fixture endurance run with repeated
64-hit batches and pause/resume cycles completed with the transport stopped
after cleanup; it is a lifecycle check, not a calibrated drift or memory
measurement. Neither headless Chrome nor simulated input measures device or
speaker latency, real-device display frame rates, or physical MIDI timing.

See `PERFORMANCE_REPORT.md` for the measured comparison. Large score import still causes an isolated main-thread task, especially at 10,000 notes. It occurs before practice, not recurrently during musical playback; renderer virtualisation should only be considered with further evidence and correctness coverage.

## Regression policy

`tests/performance/budgets.json` combines absolute interaction targets with a relative threshold, an absolute change floor, and minimum sample count. Relative comparisons are only enforced on matching OS/architecture. A single noisy sample is not authoritative. Chrome version, fixture manifest, commit, environment, sample counts and methodology are stored in every JSON report.

The comparator now also requires the same report schema, tier and scenario signature; missing required metrics, non-finite p95 values and inadequate candidate sample counts fail enforced runs. Relative gates additionally require a matching browser major version. The checked-in fast baseline is macOS/ARM and predates structured browser-version metadata, while GitHub Actions runs Linux/x64. CI therefore enforces the reliable absolute callback and score-update budgets but treats relative changes as diagnostic until a reviewed Linux/x64 baseline is recorded on the same runner/browser workload. Memory growth remains diagnostic because precise Chromium heap size is not controlled-GC leak evidence.

Headless input-to-paint results vary with animation-frame scheduling (16.5–31.9 ms p95 in the final repeated fast runs), so the 16.7 ms objective is reported but not used as a brittle absolute CI gate. Synchronous MIDI callback and score-update budgets remain absolute because they are substantially less scheduler-dependent.

## Isolated scoring feasibility

[`docs/scoring-frontier-study.md`](docs/scoring-frontier-study.md) records the opt-in native and Chrome/WASM frontier experiment, its separate scoring-policy proposal, reproduction commands and failed production gate. Its synthetic measurements are diagnostic only; the candidate is not integrated into the app and does not replace the real MIDI/audio performance harness or its budgets.

## Production scoring v4

[`docs/scoring-policy-v4.md`](docs/scoring-policy-v4.md) describes the bounded
local engine that replaced the separate live greedy and final global matchers.
The existing `midi_callback_ms` browser measurement includes Rust/WASM event
ingestion and provisional scoring; `score_update_ms` and
`feedback_paint_work_ms` cover the downstream Notation path, while
`highway.renderer_update_ms` remains the renderer-specific diagnostic. Miss
confirmation is advanced from the existing Tone frame with a 32 ms delivery
allowance and is checked in browser tests; no additional timer or animation
loop is measured. The fast, full and endurance commands remain the production
acceptance harnesses for callback tails, long tasks, heap use and audio
scheduling.

An opt-in native diagnostic exercises event ingestion, frontier advancement
and incremental feedback updates (confirmed deltas plus the current
provisional projection). One release-mode run on 2026-11-17 used macOS 25.6.0
arm64, Rust 1.99.1 and Node 22.23.1:

| Workload                       |             v4 p50/p95/p99/max ms | Final drain ms |
| ------------------------------ | --------------------------------: | -------------: |
| 500 notes, 250 ms              | 0.0042 / 0.0048 / 0.0097 / 0.0147 |         0.1369 |
| 500 notes, 62.5 ms             | 0.0119 / 0.0160 / 0.0251 / 0.0310 |         0.0638 |
| 2,000 notes, 62.5 ms, one gap  | 0.0153 / 0.0262 / 0.0286 / 0.1000 |         0.1273 |
| 10,000 notes, 62.5 ms, one gap | 0.0299 / 0.0399 / 0.0468 / 0.0655 |         0.3892 |

The historical private live-greedy measurements for the matching 500/250,
500/62.5, 2,000-gap and 10,000-gap workloads had p95 values of 0.0172,
0.0129, 0.0257 and 0.1370 ms respectively. These functions do different work:
the v4 diagnostic includes frontier confirmation and feedback deltas, uses an
exact raw note, and excludes Leptos signals, serialization, audio and browser
rendering. The comparison is therefore a native responsiveness check, not an
end-to-end latency claim. Reproduce with:

```sh
cargo run -p rudiment-core --locked --release --example scoring_v4
```

## Hardware-in-the-loop

Simulation validates Rudiment's browser event path, filtering, matching and rendering, not physical latency. Use the USB drum-kit checklist in `TESTING.md` for actual devices. Record Chrome/OS, interface and drum module, connection type, audio route, calibration, at least 100 strikes at each tested tempo, and p50/p95/p99/max arrival-to-processing and acoustic/loopback measurements. Do not merge hardware results with simulated delivery latency.
