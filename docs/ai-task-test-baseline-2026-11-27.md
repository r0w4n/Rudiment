# AI task browser baseline — 27 November 2026

## Clean `main` run

- Revision: `a11d7fe925db3a5dea99f9296b2d54dffa528974` (`origin/main`). The isolated worktree was clean before dependency installation and build.
- Environment: macOS arm64, Node 22.23.1, Rust 1.99.1, Trunk 0.22.1, Playwright 1.65.0, Google Chrome 158.0.8127.37.
- Commands: `npm ci`, `npm run build`, then one `npx playwright test --reporter=json`. The production build completed before the browser server started. No `git stash` or reused compiled assets were involved.
- Result: **144 passed, 0 failed, 0 skipped, 0 flaky** in 196.5 seconds. The JSON report was generated on 27 November 2026 at 11:52 UTC.

| Suite | Passed | Failed |
| --- | ---: | ---: |
| `accessibility.spec.ts` | 12 | 0 |
| `adaptive-restart-scroll.spec.ts` | 1 | 0 |
| `metronome-tempo-display.spec.ts` | 5 | 0 |
| `metronome-visuals.spec.ts` | 5 | 0 |
| `performance-correctness.spec.ts` | 5 | 0 |
| `presentation.spec.ts` | 18 | 0 |
| `rudiment.spec.ts` | 97 | 0 |
| `server.spec.ts` | 1 | 0 |

The timed-out [issue #23 run](https://github.com/r0w4n/Rudiment/actions/runs/45963730483) used base `98833dea9e50409ec4c948983de971813a2f4fff` and reported failures in eight existing browser tests: built-in rudiment timing/engraving, minimal playhead, playhead in bar two, selected-bar count-in, early downbeat/loop hits, loop/restart stop state, repeat-until-good retry, and bar retry tempo/pause behavior. All eight passed in this freshly built clean-main run. The old `git stash` comparison did not rebuild and provides **no evidence** that they were pre-existing. Because the old run used a Linux runner and modified issue code while this baseline used local macOS and newer `main`, classify those eight as **unresolved candidate regressions or environment-dependent failures**, not proven pre-existing. Reproduce a failing case on a clean, freshly built checkout at the same base SHA and browser environment before assigning a cause. Do not weaken an assertion, skip a test, or raise its timeout to make it pass.

## Development and CI split

During issue work, run only focused Rust and Playwright tests covering the changed path, after meaningful edits. A failing check gets one rerun after a specific fix and a short diagnosis. The pull-request quality workflows retain the full browser, accessibility, unit, build, and performance gates. Direct Playwright runs build automatically; `RUDIMENT_E2E_SKIP_BUILD=1` uses a recorded source/output fingerprint and fails if a stash, checkout, edit, or compiled-asset change made `dist/` stale.

## MIDI modal contract for issue #23

The modal does not exist on this clean `main`, so tests that require it cannot pass until issue #23 is implemented. `midi-startup.spec.ts` now covers two existing prerequisites with mocked MIDI access: an unresolved permission request leaves the workspace usable, and a successful automatic connection remains connected with no connection dialog after a virtual 3.1-second advance.

When implementing issue #23, extend that focused file with deterministic cases for:

1. Install Playwright's clock before navigation and mock `requestMIDIAccess` with a controlled unresolved promise. Assert the notation and practice controls are usable before advancing the clock; advance to just before three seconds and assert no dialog; advance past three seconds and assert the connection dialog is visible. Do not wait three real seconds.
2. Resolve the mocked MIDI promise with one connected input before three seconds. Advance past three seconds and assert the dialog never appears and the selected input is connected.
3. Leave connection unresolved past three seconds, dismiss the dialog using its real accessible close control, and assert it stays dismissed while practice controls and the current practice state remain usable. Reopening it through the MIDI connection control must still work.
4. Exercise an automatic connection that resolves after the dialog appears, and verify the UI does not steal focus or interrupt active practice unexpectedly.

Use explicit mocked input maps, permission state, and `page.clock` advancement. Keep these assertions in the PR CI browser suite once the modal exists.
