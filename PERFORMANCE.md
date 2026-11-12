# Rudiment performance engineering

Rudiment profiles its production WASM/Leptos, VexFlow, Tone.js and Web MIDI path in real Chrome. The harness is not a substitute scoring engine: fixtures enter through the same MIDI import callback, simulated device events enter through Web MIDI, keyboard events use the window listener, and feedback is observed in the rendered SVG.

## Runtime map

MIDI files are fetched/file-read in `web/js/bridge.js`, parsed and quantised by `crates/core`, stored in Leptos state, then engraved by VexFlow. During practice, Tone.Transport schedules metronome/reference audio independently of `requestAnimationFrame`. Web MIDI or keyboard events cross into the Leptos hit handler, Rust matches and classifies them, a next-frame bridge update paints noteheads/extra-hit markers, and the slower UI tick updates counters and session state. Completion runs the deterministic full-session matcher and persists the summary to IndexedDB.

| Concern                                                                        | Implementation                             |
| ------------------------------------------------------------------------------ | ------------------------------------------ |
| MIDI parsing, quantisation, tempo map, scoring                                 | `crates/core/src/lib.rs`                   |
| App state, repetition, transitions, keyboard input, persistence calls          | `web/src/main.rs`                          |
| Web MIDI, audio scheduling, notation, playhead, scrolling, feedback, IndexedDB | `web/js/bridge.js`                         |
| Real-browser workloads and reports                                             | `scripts/performance`, `tests/performance` |

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

Directly observed values include callback work, import-to-render, notation rendering, score/feedback work, input-to-DOM-mutation latency, animation intervals, Long Tasks, response times, and Chromium precise heap size. Frame drops are estimated from intervals above 16.7 ms. `AudioContext.getOutputTimestamp()` helps align scheduled visual pulses, but the harness does not measure acoustic output, USB/driver latency, compositor-only work, or GC pauses unavailable through stable browser APIs.

## Findings and optimisations

The baseline identified notation layout as the dominant long task. The 10,000-note fixture repeatedly queried the complete SVG once for every one of 625 notation systems while calculating inter-system mark clearance. A single indexed DOM traversal removed that quadratic pass without changing engraving. Live input matching was already below the 2 ms p95 target and was not refactored.

Input processing was fast but note colour feedback waited for a 32 ms application-state tick. Feedback is now coalesced onto the next animation frame; UI counters remain throttled and Tone.Transport remains independent of rendering.

See `PERFORMANCE_REPORT.md` for the measured comparison. Large score import still causes an isolated main-thread task, especially at 10,000 notes. It occurs before practice, not recurrently during musical playback; renderer virtualisation should only be considered with further evidence and correctness coverage.

## Regression policy

`tests/performance/budgets.json` combines absolute interaction targets with a relative threshold, an absolute change floor, and minimum sample count. Relative comparisons are only enforced on matching OS/architecture. A single noisy sample is not authoritative. Chrome version, fixture manifest, commit, environment, sample counts and methodology are stored in every JSON report.

The comparator now also requires the same report schema, tier and scenario signature; missing required metrics, non-finite p95 values and inadequate candidate sample counts fail enforced runs. Relative gates additionally require a matching browser major version. The checked-in fast baseline is macOS/ARM and predates structured browser-version metadata, while GitHub Actions runs Linux/x64. CI therefore enforces the reliable absolute callback and score-update budgets but treats relative changes as diagnostic until a reviewed Linux/x64 baseline is recorded on the same runner/browser workload. Memory growth remains diagnostic because precise Chromium heap size is not controlled-GC leak evidence.

Headless input-to-paint results vary with animation-frame scheduling (16.5–31.9 ms p95 in the final repeated fast runs), so the 16.7 ms objective is reported but not used as a brittle absolute CI gate. Synchronous MIDI callback and score-update budgets remain absolute because they are substantially less scheduler-dependent.

## Hardware-in-the-loop

Simulation validates Rudiment's browser event path, filtering, matching and rendering, not physical latency. Use the USB drum-kit checklist in `TESTING.md` for actual devices. Record Chrome/OS, interface and drum module, connection type, audio route, calibration, at least 100 strikes at each tested tempo, and p50/p95/p99/max arrival-to-processing and acoustic/loopback measurements. Do not merge hardware results with simulated delivery latency.
