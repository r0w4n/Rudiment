# Rudiment performance report

Generated 2026-11-12T11:31:55.281Z from commit `0c61d0129bcf411720e47567f15b5ffce5684411` (dirty worktree).

## Environment and methodology

- Tier: full; repeats: 1
- Platform: darwin 25.6.0 (arm64); 10 logical CPUs
- Runtime: v22.23.1; browser: Chrome 157.0.8103.51
- Direct observations: Browser Performance API, Long Tasks API, requestAnimationFrame, synchronous callback timings, DOM mutation latency and Chromium precise heap size.
- Estimates and limitations: Dropped-frame counts are estimated from frame intervals above 16.7 ms. Audio output and USB/driver latency are not measured.

## Scenarios

- small: 4 bars, 64 notes, 60 BPM
- medium: 30 bars, 480 notes, 120 BPM
- large: 125 bars, 2000 notes, 180 BPM
- extreme: 625 bars, 10000 notes, 240 BPM
- dense: 64 bars, 2560 notes, 240 BPM with simultaneous hits

## Measurements

Times are milliseconds except `heap_growth_bytes`, which is bytes. “Before” is the baseline p95; current percentile columns are from this run.

| Metric | Before p95 | p50 | p95 | p99 | Max | p95 change | Samples |
|---|---:|---:|---:|---:|---:|---:|---:|
| import_to_render_ms | 2363.66 | 451.20 | 1929.40 | 2221.88 | 2295.00 | -18.4% | 5 |
| notation_render_ms | 2172.18 | 387.10 | 1747.10 | 2016.94 | 2084.40 | -19.6% | 5 |
| long_task_ms | 2170.75 | 416.00 | 2212.00 | 2212.00 | 2212.00 | +1.9% | 8 |
| frame_interval_ms | 353.28 | 16.70 | 369.91 | 413.00 | 416.70 | +4.7% | 23 |
| heap_growth_bytes | 113493258.00 | 13450029.00 | 123531507.40 | 145350853.48 | 150805690.00 | +8.8% | 5 |
| midi_event_to_paint_ms | 37.95 | 9.65 | 14.13 | 16.10 | 16.80 | -62.8% | 36 |
| keyboard_event_to_paint_ms | 30.89 | 10.30 | 13.72 | 13.99 | 14.10 | -55.6% | 36 |
| scroll_call_ms | 0.00 | 0.00 | 0.00 | 0.10 | 0.10 | — | 360 |
| transition_response_ms | 120.55 | 72.00 | 148.65 | 156.13 | 158.00 | +23.3% | 12 |
| modal_response_ms | 51.75 | 39.50 | 92.70 | 99.93 | 102.00 | +79.1% | 24 |
| midi_callback_ms | 0.30 | 0.20 | 0.70 | 0.80 | 1.70 | +133.3% | 786 |
| score_update_ms | 1.50 | 0.60 | 2.67 | 2.92 | 11.00 | +78.3% | 186 |
| feedback_paint_work_ms | 1.75 | 0.40 | 1.69 | 4.63 | 10.40 | -3.7% | 75 |
| audio_scheduling_ms | 0.20 | 0.00 | 0.20 | 0.20 | 0.80 | -0.0% | 822 |
| animation_frame_interval_ms | 52.49 | 16.70 | 75.49 | 89.41 | 169.20 | +43.8% | 108 |

## Engineering interpretation

- The extreme fixture's notation render fell from 2,613.4 ms to 2,084.4 ms (20.2%), and import-to-render fell from 2,832.6 ms to 2,295.0 ms (19.0%). This is the measured effect of replacing per-system full-SVG scans with one indexed traversal.
- MIDI input-to-paint p95 fell from 37.95 ms to 14.13 ms (62.8%); keyboard input-to-paint fell from 30.89 ms to 13.72 ms (55.6%). Both now fit the initial 16.7 ms objective in this controlled run.
- MIDI callback p95 was 0.70 ms across 786 events, including the 10,000-note exercise, below the 2 ms objective. Immediate next-frame feedback raises score-update work, but its 2.67 ms p95 remains inside a 16.7 ms frame.
- Under the extreme exercise's 250-event burst, the largest feedback paint task was 10.4 ms after per-system note indexing and cached marker geometry. No recurring practice feedback task exceeded 50 ms.
- The isolated extreme import still produces a roughly 2.2 s Long Task and high peak heap use. It happens before playback, remains an open concern, and is not described as meeting the active-practice long-task target.
- The completed 15-minute endurance run captured 437 samples across 15 exercise-length loops. Heap stayed within 18.0–50.5 MB rather than increasing monotonically; the first and last active samples were 26.0 MB and 31.2 MB. After Restart, markers, scheduled voices, active voices and the transport all returned to zero. The post-stop heap was 39.6 MB, within the active range; without an exposed-GC hook it is treated as a noisy retained-heap observation, not proof of a leak.
- Input-to-paint is sensitive to headless Chrome scheduling: repeated fast-tier runs ranged from 16.5 to 31.9 ms p95, while the final full-matrix run measured 14.13 ms. CI therefore treats 16.7 ms as an engineering objective and uses repeatable relative regression margins rather than an unreliable hard paint-latency failure.

Transition/modal figures are automation response times and include Playwright round trips. The final full run also adds extreme-score interaction traffic that the baseline interaction subset did not contain, so their relative percentages are informative but not release gates.

The compact structured result is retained in `tests/performance/optimised-summary.json`; raw local reports remain reproducible under `performance-results/` and CI uploads them as artifacts.

## Reproduction

`npm run perf:fast` runs the PR-sized profile. `npm run perf:full` runs the complete workload matrix. `npm run perf:endurance` defaults to 15 minutes. Use `npm run perf:trace` for Playwright traces. Physical USB-MIDI and audible output latency require the hardware checklist in `PERFORMANCE.md`; simulated Web MIDI does not validate either.
