# Notation Hit Feedback Redesign — Progress

## Status
Implemented and validated. Follow-up round removed the remaining paint bottleneck
(SVG drop-shadow filters on feedback) after user reported continued stutter.
Third pass implemented the low-risk scaling optimisations (items 2–5 from the
review): cheap feedback diffing, painted-only clearing, cached DOM refs and
O(1) scroll row lookup. Fourth pass moved the production playhead into a
composited HTML overlay (handover item 1) and throttled the non-playhead UI
work (handover item 4, minus the metronome pulse, which is unchanged).

## Completed work
- Decoupled notation feedback painting from the transport/playhead frame.
- Cached notehead geometry during `renderScore` so feedback halos/markers no longer force SVG layout (`getBBox`/`getCTM`) on hits.
- Removed flashing (`excellent-flash` toggling) from feedback painting.
- Guarded feedback painting with `visualHitsEnabled` query flag (diagnostic toggle).
- Preserved halos, accuracy indicators, missed/unmatched markers, positioning and timing.

## Investigation findings / bottlenecks
- `applyJudgements()` ran synchronously inside `updateScore()` during the transport animation frame.
- Each hit forced layout via `head.getBBox()` (halos) and `noteHeadY()` `getBBox()`/`getCTM()` (unmatched markers).
- `markers.replaceChildren()` rebuilt all unmatched markers each projection.
- `JSON.stringify(judgements)` signature check and incremental diff already limited work, but layout reads blocked the main thread and caused simultaneous playhead skips.

## Architectural decisions
- Feedback now queued via `queueScoreFeedback()` (rAF) instead of applied in `updateScore()`, so playhead/scroll update first.
- Geometry cached in `meta.headBounds` and `meta.headCenterY` at render time.
- Flashing removed; accuracy data attributes and halos retained.

## Files modified
- `web/styles.css`
  - Removed drop-shadow filters from notation judgement halos and unmatched markers.
  - Removed unused excellent-flash animation/keyframes.
  - Highway feedback CSS left unchanged.
- `web/js/bridge.js`
  - `applyJudgements`: removed flashing toggle.
  - `paintExpectedJudgement`: use cached `meta.headBounds`, fallback to `getBBox`.
  - `noteHeadY`: use cached `meta.headCenterY`.
  - `updateScore`: queue feedback instead of synchronous apply.
  - `queueScoreFeedback`: respect `visualHitsEnabled`.
  - `renderScore`: cache head bounds/center Y.
- `NOTATION-FEEDBACK-PROGRESS.md`

## Tests performed
- `npm run bundle` + trunk build: success.
- `cargo test --workspace`: 28 passed, 1 ignored.
- `npx playwright test tests/e2e/performance-correctness.spec.ts`: 4 passed.

## Outstanding issues
- Subjective smoothness at 80 BPM not measured with frame timings here; perf harness should be rerun for before/after.
- In-progress git revert of commit 27c0285 remains in working tree; no commits made.

## Next steps
- Run `npm run perf:fast` and compare feedback_paint_work_ms / animation intervals.
- Validate halos/misses/accuracy manually in browser at 80 BPM.
- Clean up temporary diagnostic code (`playhead-debug`, `visual-hits`, `metronome-animation`) only after diagnostic needs end.
## Follow-up: user still perceived stutter (second pass)
Root cause: feedback CSS used `filter: drop-shadow(...)` on every judgement halo
and unmatched marker. SVG filters force expensive offscreen rasterisation; once
a halo existed, every playhead `x1` mutation repainted the filtered layer, which
stalls both diagnostic playheads simultaneously while Highway (plain rects) stayed smooth.

Changes:
- Removed `filter: drop-shadow(...)` from perfect/good/early/late halos and
  unmatched-hit markers in `web/styles.css`.
- Removed now-unused `excellent-hit` keyframes and `.excellent-flash` rule.
- Kept halo fills/opacity, notehead colours, marker fill/stroke and all semantics.

Measured with temporary Playwright 80 BPM run, medium fixture, 66 MIDI hits over 4 s:
- visual feedback off: rAF p95 16.8 ms, max 16.8 ms.
- visual feedback on: rAF p95 16.7 ms, max 16.8 ms; feedback paint p95 0.5 ms,
  score update p95 0.4 ms, 68 feedback nodes present.
- Before removing filters the harness did not capture compositor rasterisation,
  so this measurement complements rather than replaces real-browser visual checks.

Temporary measurement spec was deleted. Rebuilt `dist` and reran
`tests/e2e/performance-correctness.spec.ts` (4 passed).

Next action if stutter remains on real hardware: record a Chrome performance
trace while feedback is visible and inspect Paint/Raster/Composite durations for
the filtered SVG layer; consider moving halos to a separate lightweight overlay.

## Third pass: scaling optimisations (items 2–5)
Completed in `web/js/bridge.js`:

2. **Cheap feedback diffing**
   - Removed the full-projection `JSON.stringify(judgements)` signature.
   - `applyJudgements` now returns early on reference equality
     (`judgements === scoreData.lastJudgements`); Rust sends a new array only
     when feedback changes.
   - Added `sameJudgement(left,right)` comparing only feedback-relevant fields
     instead of serialising each judgement twice.
   - Removed the obsolete `judgementSignature` field.

3. **Clear only painted feedback**
   - Added `scoreData.paintedGroups` (`Set`), populated by
     `paintExpectedJudgement`.
   - `clearScoreFeedback` now clears only those groups instead of walking every
     note in the score. This matters on large scores at loop boundaries and restart.

4. **Cache feedback DOM lookups**
   - Cached `meta.group`, `meta.noteheads`, `meta.notehead` and `meta.headText`
     during `renderScore`, alongside the existing geometry cache.
   - Cached the `#unmatched-markers` layer as `scoreData.markerLayer`.
   - `clearExpectedJudgement`, `paintExpectedJudgement` and `noteHeadY` use the
     cached refs; no per-hit `querySelector` tree walks.
   - Unmatched markers are only rebuilt when the marker list or existing DOM
     changes, and the `judgements.filter(...)` allocation was removed.

5. **O(1) scroll row lookup**
   - `cameraAtBeat` now reads `scoreData.bars[position.row]` instead of scanning
     `bars.find(...)` every frame (`barsPerLine` is always 1).

### Test updates
- `tests/e2e/rudiment.spec.ts`
  - Removed the obsolete `excellent-flash` and `drop-shadow` expectations.
  - Feedback-update tests now await the queued animation frame before reading
    DOM, matching the intentionally asynchronous feedback path.
  - Unmatched-marker precision test now waits for the marker count before
    measuring positions.

### Validation for third pass
- `npm run bundle` + Trunk build: success.
- `npx playwright test tests/e2e/performance-correctness.spec.ts`: 4 passed.
- `npx playwright test tests/e2e/rudiment.spec.ts`: **88 passed** (2.7 min),
  including feedback, playhead, unmatched-marker, restart and notehead tests.
- `npm run qa:browser -- --skip-build`: passed (159.9 s).
- `npm run qa:a11y -- --skip-build`: passed (12.2 s).
- `npm run qa:performance -- --skip-build`: passed (15.6 s).
- `npm run lint`: passed (ESLint + TypeScript).

## Current repository state / handover
- Earlier filter/feedback work was committed by the user as `b6bb274`
  ("applying fixes to reduce stutter") during this pass.
- The third-pass work (items 2–5, test updates and this document) is currently
  **uncommitted** in the working tree. No commits, pushes, stashes or reverts
  were performed by the agent.
- Modified/uncommitted: `web/js/bridge.js`, `tests/e2e/rudiment.spec.ts`,
  `NOTATION-FEEDBACK-PROGRESS.md`.

## Current next steps
- See **Remaining playhead smoothness work (detailed handover)** below.
- Highest priority: move the production playhead out of the full score SVG into
  a composited overlay.
- Then re-measure before doing lower-priority scroll/UI throttling.
- Real-browser subjective validation at 80 BPM remains the final check.

## Remaining playhead smoothness work (detailed handover)

The feedback path is now cheap enough that the remaining risk is mostly
**paint/raster work caused by the playhead itself**, plus a few per-frame UI
updates. These are not implemented yet. Keep scoring, MIDI processing, the Tone
transport and Highway mode unchanged.

### 1. Move the production playhead out of the full score SVG — highest impact
**Current:** `web/js/bridge.js:1199–1204` mutates `x1`/`x2` on the `<line>` inside
the complete notation SVG every frame. That invalidates and repaints the whole
score SVG, even on large exercises.

**Evidence:** the opt-in diagnostic playhead (`playheadDebug.attachContainer` /
`paint`, around `web/js/bridge.js:960–1020`) already uses an absolutely
positioned HTML overlay and `transform: translate3d(...)`. It is updated with
`top`/`height` only when the system row changes, which is the compositor-friendly
shape we want for production.

**Suggested implementation:**
- Add a dedicated overlay element inside `.notation` (HTML div or separate SVG
  layer) and make it the visible playhead.
- Per frame: update only `transform: translateX(x)`.
- On row change: update `top` and `height` once, not every frame.
- Use `will-change: transform` and `pointer-events: none`.
- Keep the existing SVG `<line id="playhead">` hidden if tests/accessibility
  need `#playhead`, `data-x`, `data-row`, `data-bar`; otherwise move those
  attributes to the overlay and update tests.
- Respect the existing WebKit warning at `web/js/bridge.js:1196–1198`: changing
  transform and row geometry every frame could freeze the composited layer.
  Updating row geometry only at row boundaries should avoid that; verify at
  system transitions.

**Risk:** medium–high because several tests observe `#playhead` attributes and
`x1` mutations. **Expected gain:** largest remaining improvement, especially on
large/extreme scores.

**Validation:** 80 BPM with feedback on/off; medium, large and extreme fixtures;
row-boundary transitions; pause/resume/restart; dark notation theme; compare
`#playhead`/diagnostic positions and rAF intervals.

### 2. Separate feedback from the score SVG as well (if needed after item 1)
Even with an overlay playhead, halos and unmatched markers still mutate the score
SVG. If dense feedback still causes raster work, move feedback into the same
lightweight overlay layer using the cached geometry (`meta.headBounds`,
`meta.headCenterY`, `positionAtBeat`). This avoids mutating the engraved score
tree during playback.

**Risk:** medium — halos must stay aligned with noteheads and must survive
resize/theme changes. **Expected gain:** medium for dense simultaneous hits.

### 3. Reduce row-transition scroll cost
**Current:** `updateScore` sets `container.scrollTop` during system transitions
(`web/js/bridge.js:1216`). This is necessary only at row boundaries, but it can
force layout/paint and fires the passive scroll listener. If profiling shows
stutter at bar/row boundaries, consider:
- precomputing row scroll targets (already mostly done via `cameraAtBeat`),
- using a transform-based inner scroller instead of `scrollTop`, or
- keeping `scrollTop` but ensuring only one write per frame and no reads after it.

**Risk:** medium–high because manual scrolling, focus, accessibility and selected
bar navigation depend on the existing scroll container.

### 4. Throttle/memoise non-playhead UI work
- `web/src/main.rs:1988` sets `progress` every ~32 ms. That re-runs the bar label
  and timeline width views even when the displayed bar has not changed.
  Memoise the bar label and update the thin timeline at ~10 Hz while leaving
  scoring in the existing frame callback.
- `web/js/bridge.js:185` sets the metronome CSS custom property every frame, and
  pulses animate `box-shadow`. This is secondary, but can be updated at 30 Hz or
  moved to a direct transform.
- `performanceProfile.record` (`web/js/bridge.js:82`) grows unbounded arrays in
  perf mode. Use a ring buffer/cap for endurance runs so measurement does not
  add GC noise.

**Risk:** low. **Expected gain:** low–medium, mostly long-session/UI headroom.

### 5. Measurement checklist
Do not treat headless rAF timings as proof of real-device smoothness. Use the
existing diagnostic playhead and a real Chrome performance trace:
- frame intervals and skipped frames at 80 BPM,
- Paint/Raster/Composite duration for the notation SVG,
- `score_update_ms`, `feedback_paint_work_ms`,
- feedback on vs off, medium/large/extreme fixtures,
- row transitions, dense simultaneous hits, pause/resume/restart.

The existing `playhead-debug` diagnostic (`?playhead-debug=1`) exports CSV via
`rudiment.exportPlayheadDebugCsv()` and compares the green production playhead
with the blue reference overlay.

## Fourth pass: composited playhead overlay and throttled per-frame UI

Implements handover items 1 and 4 (except the metronome pulse sub-item) from
the "Remaining playhead smoothness work" handover above. Items 2 (feedback
overlay) and 3 (scroll cost) remain deferred until the overlay is measured on
real hardware.

### 1. Composited playhead overlay (highest impact)
- `renderScore` now also appends a `.playhead-overlay` HTML div (aria-hidden)
  inside `#notation`, which becomes the visible playhead.
- Per frame `updateScore` writes only `transform: translate3d(x, 0, 0)` on the
  overlay — a compositor-only change that no longer invalidates the engraved
  score SVG. Overlay `top`/`height` are written only at row boundaries, the
  shape the handover prescribed for WebKit safety.
- The SVG `<line id="playhead">` stays in the DOM but is styled
  `visibility: hidden` and never paints. It keeps mirroring `x1`/`x2`, `y1`/
  `y2`, `data-x`, `data-row`, `data-bar`, `data-beat` and the `visible` class
  exactly as before, so every existing test, the `playhead-debug` diagnostic
  and the historical WebKit-freeze workaround stay valid.
- `#notation` gains `position: relative` so the overlay is positioned in the
  scrolled score coordinate space (same technique the diagnostic overlay uses).
- Overlay CSS mirrors the old line: 3px `#14896d`, `opacity .85` when visible,
  `will-change: transform`, `pointer-events: none`.

Residual risk: the WebKit layer-freeze warning in the handover applies to
geometry changes on composited layers. Overlay geometry now changes only at
row boundaries (not per frame), which the handover expected to be safe, but
this must be confirmed on real iPad hardware in Safari — headless Chrome tests
cannot establish that.

### 4. Throttled non-playhead UI work
- `web/src/main.rs`: new `displayed_bar` Memo derives the one-based bar index
  from `progress` and `bar_sequence`; the bar label view now runs only when
  the displayed bar (or selection/attempt state) actually changes instead of
  on every ~32 ms transport frame.
- New `timeline_beat` signal samples the same beat for the thin timeline bar
  at ~10 Hz (plus a forced update on a run's final frame); pause/reset/launch
  sites update it immediately so the resting bar stays correct.
- `performanceProfile.record` now bounds each metric's sample history
  (4096, dropping the oldest quarter when full) so endurance runs no longer
  accumulate unbounded arrays and their GC noise.
- The metronome CSS custom property (`bridge.js` `--metronome-swing`) is
  unchanged: it drives the pendulum animation and is secondary.

### Test additions
- `tests/e2e/rudiment.spec.ts`: new deterministic test asserting the overlay
  is the visible playhead (hidden SVG line, `will-change: transform`), moves
  only by `transform` within a row, writes geometry only at row boundaries,
  and mirrors the hidden line's `data-x`.
- The continuous-motion playback test now also asserts the SVG line stays
  `visibility: hidden` while the overlay carries the `visible` class.

### Validation (fourth pass)
- `npm run lint` (ESLint + TypeScript): passed.
- `cargo test --workspace --locked`: 28 passed, 1 ignored.
- `npm run qa:quick`: passed (formatting, cargo check, clippy).
- `npm run qa:unit`: passed (Rust + JS unit tests).
- `npx playwright test tests/e2e/performance-correctness.spec.ts`: 4 passed.
- `npx playwright test tests/e2e/rudiment.spec.ts`: 90 tests, all passing
  (also green inside `qa:browser`).
- `npm run qa:browser -- --skip-build`: passed (156.6 s, full e2e set).
- `npm run qa:a11y -- --skip-build`: passed (11.1 s).
- `npm run qa:performance -- --skip-build` (`perf:fast:measure` with
  baseline `--enforce`): passed all absolute gates.
- Diagnostic (not strict) comparison vs `tests/performance/baseline.json`,
  same machine but baseline recorded on an older Chrome: `score_update_ms`
  p95 1.10 → 0.10 ms, `midi_event_to_paint_ms` p95 38.96 → 14.67 ms;
  `midi_callback_ms` 0.30 → 0.50 ms (0.2 ms absolute). Treat as direction,
  not proof — per project policy these are headless numbers and cannot
  establish real-device smoothness.

### Pre-existing flake observed (not caused by this pass)
`Restart clears painted note feedback` (rudiment.spec.ts:134) intermittently
fails in full-suite runs on this machine: at 240 BPM the
`.score-note[data-judgement]` count passes through its expected value 1 for
only ~100–200 ms and a loaded Playwright poll can skip it (observed 0 → 2).
Reproduced identically on unmodified `main` (c57bb93) built in a separate
worktree, and passes 4/4 in isolation. Left unfixed as out of scope; a
tolerant poll (e.g. `expect.poll(...).toBeLessThanOrEqual(1)` bounded by a
frame wait) would remove the flake.

### Remaining after this pass
- Handover item 2 (move feedback halos/markers into the overlay layer) and
  item 3 (row-transition scroll cost) stay deferred until the overlay is
  measured on real hardware.
- The metronome per-frame CSS custom property is unchanged.
- Real-iPad/Safari validation of row-boundary overlay geometry changes (the
  WebKit composited-layer freeze risk) is still outstanding; headless Chrome
  tests cannot cover it.



## Fifth task: residual micro-jitter reference demo comparison

User report: after the feedback redesign the playhead is much smoother, but a
subtle residual remains - small synchronised micro-stutters of the green
production playhead and the blue `?playhead-debug=1` reference, occasionally
hesitating then advancing slightly further than expected; visible without
MIDI hits and at 30 BPM; unaffected by muting the metronome. A previously
created standalone demo (`~/Downloads/playhead-smoothness-full-width.html`)
appeared smoother but was never compared on the same iPad.

Plan (constraints: no commits/pushes/merges/stashes/reverts, no speculative
production changes, deliver evidence only):

1. Integrate the standalone demo as a self-contained, dependency-free page at
   `web/public/animation-demo.html`, preserving its behaviour (three movement
   methods, fit-to-width + fullscreen, thickness, metrics), adding only
   URL-parameter preconfiguration, a frame/position trace with CSV export
   comparable to the app's playhead-debug CSV, and a Safari fullscreen
   prefix fallback.
2. Route `?animation-demo=1` to it via a tiny inline redirect in
   `web/index.html`, before the WASM bundle or bridge loads. Production
   behaviour is untouched when the parameter is absent.
3. Build and run regression checks; confirm no production behaviour changed.
4. Measure both the demo and Rudiment (`?playhead-debug=1`) locally in
   Chromium and WebKit at identical viewport/BPM, comparing objective
   regularity metrics; lay out candidate causes and confidence without
   prescribing a fix.
5. Leave everything uncommitted in the working tree and record findings here
   for on-iPad comparison.

### Implementation (complete, uncommitted)

- `web/public/animation-demo.html` (new): the original
  `~/Downloads/playhead-smoothness-full-width.html` preserved verbatim in
  behaviour (three movement methods, fit-to-width, fullscreen, thickness,
  metrics, 4-beat bar crossing at 240000/bpm ms), with additive changes only:
  - URL-parameter preconfiguration (`bpm`, `method`, `fit`, `width`,
    `thickness`) and auto-start when preconfigured, so comparison runs are
    reproducible with no interaction.
  - Per-frame trace (timestamp, interval, position, increment), bounded at
    4096 frames, exposed as `window.demoTrace()` / `window.demoTraceCsv()`
    plus an "Export trace CSV" button; on iPadOS use
    `copy(demoTraceCsv())` from a tethered console.
  - `webkitRequestFullscreen`/`webkitFullscreenElement` fallbacks for
    Safari. The `native` method is not traced (reading its resolved position
    per frame would add layout work and skew the trace); noted in the page.
- `web/index.html`: three-line inline redirect to
  `./public/animation-demo.html` (query string preserved) only when
  `?animation-demo=1` is present, before the bridge/WASM loads. No effect
  otherwise; production playhead, scoring, feedback, scrolling and Highway
  paths untouched.
- Probe `artifacts/jitter-compare.mjs` (git-ignored) drives both pages in
  Chromium and WebKit at identical viewport (1180x820 = iPad Air landscape)
  and BPM, with results in `artifacts/jitter-compare.json`.

### Measurements (local, macOS ARM, headless)

Chromium 158: demo and Rudiment both perfect (p50/p95 16.7 ms, max 16.8 ms,
zero stalls) at 30 and 80 BPM, transform and svg demo methods.

WebKit (same machine, same session, paired runs):
- Demo, transform method, 3px line, fit width: 30 BPM p50/p95 17 ms,
  max 22 ms, **0 intervals > 25 ms**; 80 BPM max 18 ms, 0 stalls.
- Demo, **svg method** (x1/x2 attribute writes every frame - the movement
  shape closest to the app's hidden mirror): **0 stalls** at both BPMs.
- Rudiment (`?playhead-debug=1`, count-in 0, no hits): 30 BPM 22 stalls
  > 25 ms (p95 21, max 32); 80 BPM 21 stalls (p95 23, max 33). Stalls are
  single-frame delays (26-33 ms) - at 30 BPM that is a ~4.9 px jump instead
  of ~2.45 px, i.e. exactly the reported "hesitate, then advance slightly
  further" micro-stutter. Reproduced across two paired runs; the standalone
  demo measured clean in the same sessions.

Stall localisation (from the diagnostic trace only, no production changes):
- Camera/row-transition windows cover 24.9% of frames; stalls inside them:
  4-5 of 21-22 - i.e. uniform. Row-transition scroll/raster is **not** the
  driver.
- Even/odd frame-index parity (proxy for the ~32 ms state tick):
  14/8 at 30 BPM, 10/11 at 80 BPM - no consistent signal; the in-frame WASM
  tick is **neither confirmed nor excluded** by this crude test.

Interpretation: the residual jitter is **not** purely environmental. On the
same WebKit engine and machine, the app misses ~0.6 frames/s under ordinary
load while a minimal rAF loop (with or without SVG attribute writes) does
not. The app's transport frame callback carries more per-frame main-thread
work than the demo, and on a weaker iPad Air core with a busier compositor
the same cost exceeds the frame budget more often.

### Candidate causes and confidence

1. Aggregate per-frame main-thread cost of the transport frame callback
   (Tone ticks computation, updateScore incl. hidden mirror + overlay,
   visual metronome, every-other-frame WASM/UI tick) - **medium-high** as a
   class (measured app-vs-demo contrast on WebKit); the specific component
   is unidentified.
2. In-frame 32 ms scoring/UI tick (WASM advance + Leptos signals) -
   **low-medium**; parity test inconclusive, cost measured small on desktop
   but unmeasured on-device.
3. Per-frame SVG mirror writes - **low**; the demo's svg-method control is
   clean on WebKit, so this pattern alone does not stall frames.
4. Row-transition scroll/raster - **low**; stalls distribute uniformly vs
   the 24.9% camera-window share, and the user observes them while
   stationary.
5. Position/clock math - **ruled out**; the wall-clock blue reference
   stutters identically to green.
6. iPadOS-only compositor/display-link irregularities - **untested
   locally**; the same-device demo-vs-app comparison exists precisely to
   separate this from (1).

### On-iPad comparison protocol

1. Deploy or serve the current build; open
   `/?animation-demo=1&bpm=30&method=transform&fit=1&thickness=3`
   (auto-starts) and Rudiment `/?playhead-debug=1` at 30 BPM, count-in 0,
   same orientation/fullscreen, no hits.
2. Watch both: if the demo also micro-stutters on the iPad, the residual is
   environmental (candidate 6) and no app change will remove it; if the
   demo is smooth while Rudiment stutters, candidate 1 stands and per-frame
   cost reduction is justified with a trace-first approach.
3. Capture CSVs from a tethered Safari console:
   `copy(demoTraceCsv())` and `copy(rudiment.playheadDebugCsv())`.
   Compare interval p95/max and increments for the hesitate-then-advance
   pattern. Optional: Safari Web Inspector Rendering Frames to attribute
   long frames.
4. Repeat at 80 BPM; also try the demo's `native` method - if the native
   compositor-driven animation is smooth while rAF-driven ones stutter on
   the iPad, that points at rAF/compositor pacing rather than main-thread
   cost.

### Differences from the original standalone demo

Behaviour preserved: methods, fit/fullscreen, thickness, metrics, bar
timing. Added: URL preconfiguration + auto-start, frame trace + CSV export,
Safari fullscreen prefixes, a title/note mentioning the in-app diagnostic
context. No Rudiment code is loaded by the demo page.

### Validation

- `npm run build`: success; `dist/public/animation-demo.html` and the
  redirect present in `dist/index.html`.
- Smoke via probe: redirect works, demo auto-starts, app unaffected without
  the parameter.
- Focused e2e (playhead, practice-time, performance-correctness,
  first-use, notation toggles): 14 passed.
- `npm run qa:quick`: passed (after removing an unused variable from the
  git-ignored probe).
- `npm run qa:browser -- --skip-build`: passed (161 s).
- Limitations: headless macOS measurements cannot establish real-device
  frame rates; the WebKit-vs-demo contrast is engine-level evidence only.
  No on-device measurement has been taken yet.

### Current repository state / handover

- Uncommitted (working tree, no commits/pushes/merges/stashes/reverts
  performed): `web/index.html`, `web/public/animation-demo.html` (new),
  `NOTATION-FEEDBACK-PROGRESS.md`.
- `artifacts/jitter-compare.mjs` and `artifacts/jitter-compare.json` are
  git-ignored tooling/results kept for resumption.
- Pre-existing stash `stash@{0}` (`preserve-local-work`) was left
  untouched.
- Next step: run the on-iPad comparison protocol above; only after that
  evidence should any per-frame cost reduction be attempted (the
  performance-correctness suite constrains how - see the reverted
  mirror-throttle attempt recorded in PERFORMANCE.md).

## Sixth task: Settings diagnostics + iPad findings

User's iPad findings (2022 iPad Air):
- The deployed reference animation demo is **completely smooth** in both
  Safari and Chrome on the iPad, while Rudiment's notation playhead still
  micro-stutters at 30 BPM without MIDI hits. Environmental/device causes are
  therefore largely excluded; the residual is app-specific per-frame work.
- A valuable unreproducible observation: after navigating away from Safari and
  returning, the app entered an abnormal state - the playhead kept moving,
  other playback functionality appeared to stop, and the playhead became
  completely smooth. Something that normally runs alongside the playhead may
  contribute to the jitter; no subsystem was assumed responsible.
- `playhead-debug=1` still shows small synchronised green/blue micro-stutters,
  unchanged by muting the audible metronome.

### Implemented (branch `feat/diagnostics-settings`)

A clearly separated "Playhead diagnostics" section at the bottom of the
Settings drawer, so iPad experiments no longer need URL editing:

- Reference playhead overlay + trace (was `?playhead-debug=1`) - runtime
  toggleable, including mid-run; the underlying `playheadDebug` object now
  always remembers the active run so Settings can enable the overlay during
  playback, and `pause`/`hide`/`attachContainer` lost their enabled guards.
- Visual hit feedback (was `visual-hits=0`/`feedback=0`).
- Metronome beat lights (was `metronome-animation=0`).
- Auto-scroll camera - new; gates the per-frame `scrollTop` writes in
  `updateScore` only.
- Scoring & UI updates - new; gates the ~30 Hz `onFrame` WASM scoring/state
  tick inside the transport frame. Transport, playhead and completion keep
  running. This deliberately reproduces the reported abnormal state
  (playhead-only) as an experiment.
- Performance counters (was `__RUDIMENT_PERF__`).
- "Minimal playhead-only" preset (tick/feedback/beat lights off, camera and
  reference overlay on) and "Reset diagnostics" (restores the URL-seeded
  defaults; a plain reload is normal operation).
- Link to the deployed reference animation at the current tempo
  (`./public/animation-demo.html?bpm=...`).

Safety: the switches are session-only (never persisted with settings, never
seeded into saved profiles). A run whose scoring/UI tick was disabled - from
its start or part-way - is marked invalid: `finish_run` returns to idle
without touching the scorer (which may never have been created), never
saves, compares or shows statistics, and shows no error. URL parameters still
seed the switches, and existing URL-based tests are unchanged. With the tick
disabled, the bridge performs the final playhead hide at completion itself.

Bridge surface: `rudiment.diagnosticsSnapshot()`,
`rudiment.setDiagnostic(name, value)`, `rudiment.resetDiagnostics()`.
Rust externs `set_diagnostic`/`diagnostics_snapshot` seed drawer state on
mount.

### Validation

- New e2e: settings diagnostics toggle the reference overlay at runtime and
  reset restores defaults; minimal playhead-only keeps the transport moving,
  completes, hides the playhead, shows no error, saves no session, and
  resetting restores saving. 3 passed.
- Regression set (playhead, practice-time, feedback, dense traffic, restart):
  19 passed plus the documented pre-existing 'Restart clears painted note
  feedback' flake, reproduced identically (1/3) on unmodified main with its
  own dist.
- `npm run build`, `qa:quick`, `qa:browser -- --skip-build` (clean rerun
  after one four-failure audio-timing flake run; all four passed in
  isolation), `qa:a11y -- --skip-build`, `perf:fast` enforced budgets: all
  passed. Perf numbers unchanged from the previous accepted run
  (midi_callback p95 0.50 ms, score_update p95 0.10 ms).

### Investigation notes - what each switch isolates

The iPad demo result (smooth reference vs stuttering app on the same device,
same browser) plus the macOS WebKit contrast (app ~0.6 stalls/s, demo zero)
confine the cause to the app's per-frame main-thread work. Ranked candidates
and what the switches test on-device:

1. In-frame ~30 Hz scoring/UI tick (WASM advance, Leptos signal updates) -
   the abnormal smooth state is exactly this configuration, so it is the
   leading hypothesis. Test: "Minimal playhead-only" at 30 BPM; if smooth,
   re-enable the tick alone. Confidence: medium (state-tick-off matches the
   observed abnormal state; parity localisation was inconclusive).
2. Metronome beat lights, auto-scroll camera, visual feedback - secondary
   per-frame/event work; toggles allow one-at-a-time re-enablement.
3. Hidden-SVG mirror writes - cannot be toggled: the performance-correctness
   suite samples `data-beat` per frame (see PERFORMANCE.md). Known residual.
4. Tone/audio subsystem - not toggleable without stopping the clock; the
   audible metronome was already excluded by the user (muting made no
   difference).

Suggested iPad protocol: Settings -> "Minimal playhead-only" at 30 BPM and
observe; then re-enable switches one at a time (tick first, then beat
lights, camera, feedback) until the stutter returns, exporting
`rudiment.exportPlayheadDebugCsv()` traces at each step if possible. If even
the minimal configuration stutters, the remaining suspect is the transport
frame's baseline work (Tone ticks computation + updateScore) or
compositor-side effects, which would need a Safari Web Inspector
Rendering-Frames trace on device.

Status: implemented and validated; committed via the standard worktree flow.

## Seventh task: fix the confirmed stutter (deferred scoring tick, cheaper beat lights)

On-device bisection (iPad) confirmed: with all optional subsystems off the
playhead is exceptionally smooth; re-enabling "Scoring & UI updates" causes
the most significant stutter and "Metronome Beat Lights" a noticeable one,
at repeatable score positions, without MIDI hits. Local WebKit reproduced the
combined effect: at 30 BPM the app missed 15-25 frames per 32-second run
with everything on, while the standalone demo missed none (probe in
artifacts/tick-bisect.mjs, results artifacts/tick-bisect.json).

### Root cause 1 - scoring/UI tick inside the animation frame (highest impact)

The ~30 Hz WASM scoring/state tick ran inside the transport frame's rAF
callback, so its cost (scoring advance at note windows, Leptos signal
updates, serialisation of the growing live-judgement vector) delayed every
frame's paint commit - at note positions and bar turns, matching the
"repeatable positions" report.

Fix: the transport frame still decides WHEN scoring work is due, but the
work is dispatched as a post-commit task (setTimeout(0) from the rAF
callback runs after that frame's paint). Scoring watermarks are monotonic,
so when ticks queue up only the newest pending tick runs - no scoring state
can be lost. flushPendingTick() is called before run teardown (completion,
pause, stop, restart), so ordering is preserved. The tick's Rust side now
sends feedback via the existing queue_score_feedback (like the MIDI input
turn) instead of update_score: a post-commit tick must not reposition the
playhead, which the transport frame owns. The update_score extern lost its
last Rust caller and was removed; the JS updateScore export for tests is
unchanged.

Regression caught by the non-contiguous-bars browser test (4/4): the old
synchronous initial tick's update_score used to pre-position the notation
mirror at the run's opening bar before the first animation frame. The
bridge now does that one deliberate pre-position itself
(scoreBeatForDisplay extracted from the frame loop; identical mapping, and
the playhead stays hidden for count-in beats exactly as before).

### Root cause 2 - metronome beat lights

Per-frame: render() built a template string 60 times per second purely to
compare displayed state (GC churn on tight devices); pulse() re-queried and
spread the beat dots on every beat. Per-beat: the pulse WAAPI animation
interpolated box-shadow, which repaints on WebKit for the animation's 180 ms.
Fixes: numeric component comparison instead of the string, cached dot list
(invalidated if the markup is rebuilt), and a static glow (inline box-shadow
for the pulse's duration, guarded against overlapping pulses) while the
animation keeps transform/opacity keyframes. Visual note: the glow now
appears at full strength for the pulse's duration and fades with the dot's
opacity, instead of interpolating shadow values over 180 ms - same timing,
scale and fade, minutely different glow ramp-in.

### Measured result (WebKit, 1180x820, 30 BPM, 32 s runs)

| Configuration        | before fix            | after fix           |
|---------------------|-----------------------|---------------------|
| all-on              | 15-25 stalls >25 ms   | 0-3 stalls (noise)  |
| state-tick only     | p95 20, max 24         | p95 19, max 20-22   |
| all-off (baseline)  | 2-3 stalls            | 0-1 stalls          |

The full configuration now measures at the same level as the deliberately
minimal diagnostic baseline. Headless macOS WebKit cannot establish
real-device smoothness; on-iPad confirmation with the existing Settings
diagnostics is the final check.

### Validation

- Focused suites (24 tests incl. per-frame data-beat timing, non-rewind,
  non-contiguous jumps 3/3, missed notes, practice time, diagnostics
  toggles): passed.
- The deterministic start-position regression was found by qa:browser and
  fixed before commit (see above); clean qa:browser afterwards.
- qa:quick, qa:browser, qa:a11y: passed. perf:fast enforced budgets passed
  (midi_callback p95 0.50 ms, score_update p95 0.10 ms, frame intervals
  unchanged).
- The scoring path itself is untouched: same engine, same watermarks, same
  feedback projection - only WHEN/WHERE the tick executes changed.

### Remaining work

- On-iPad verification: normal configuration at the previously problematic
  positions; compare against the all-off baseline with the Settings
  diagnostics if anything remains.
- The live-judgement vector is still serialised in full per feedback change
  (now off the frame's critical path); reduce only if the iPad retest still
  shows cost.
- The hidden-SVG mirror still writes every frame (test contract, see
  PERFORMANCE.md); unchanged by design.
