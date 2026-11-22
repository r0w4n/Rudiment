# Notation Hit Feedback Redesign — Progress

## Status
Implemented and validated. Follow-up round removed the remaining paint bottleneck
(SVG drop-shadow filters on feedback) after user reported continued stutter.
Third pass implemented the low-risk scaling optimisations (items 2–5 from the
review): cheap feedback diffing, painted-only clearing, cached DOM refs and
O(1) scroll row lookup.

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
