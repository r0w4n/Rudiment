# Notation Hit Feedback Redesign — Progress

## Status
Implemented and validated. Follow-up round removed the remaining paint bottleneck
(SVG drop-shadow filters on feedback) after user reported continued stutter.

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
