# Rudiment agent instructions

## Architecture and generated assets

- `crates/core/src/lib.rs` is the web-independent MIDI/timing/scoring domain; `web/src/main.rs` is the Leptos/WASM UI and practice state. `web/js/bridge.js` handles Web MIDI, Tone.js audio, VexFlow notation and IndexedDB. Tone.Transport is the authoritative musical clock; rendering must not become a second scheduler.
- Edit `web/js/bridge.js`, not the generated `web/public/bridge.js`. `npm run build` regenerates fixtures and bundles the bridge before Trunk writes `dist/`; `npm run dev` does the same and watches sources into `web/dist/` (port 5173, override with `RUDIMENT_PORT`).
- `scripts/generate-fixtures.mjs` generates ignored MIDI files and the performance manifest; the built-in `web/public/exercises/rudiments/*.mid` are tracked. Their test-only reference engravings and manifest are in `tests/fixtures/rudiment-reference/`; keep them out of the deployed app.
- GitHub Pages builds with `--public-url "./"`; preserve relative asset paths so the app also works under a repository subpath.

## Presentation architecture and rendering boundaries

The application separates musical domain logic, renderer-independent presentation state and visual rendering. Preserve this separation when implementing the MIDI highway or changing existing notation.

### Domain and application state

- `crates/core/src/lib.rs` owns web-independent MIDI exercise data, expected hits, timing, instrument classification and scoring.
- `web/src/main.rs` owns Leptos application/practice state and connects it to the browser bridge.
- Tone.Transport remains the single authoritative musical clock. Do not introduce a second clock, MIDI parser, scheduler or scoring engine for a visualisation.
- Domain data and musical behaviour must not depend on SVG, Canvas, PixiJS, DOM elements or viewport coordinates.

### Renderer-independent presentation integration

- `web/js/highway-controller.mjs` provides the renderer-independent presentation controller.
- `web/js/presentation-host.mjs` hosts the optional presentation integration.
- `web/js/bridge.js` supplies existing browser integration and transport updates.
- `docs/presentation-integration.md` documents the established integration boundary; consult it before changing the controller or adding a renderer.
- The controller/host consume authoritative exercise, selection, playback and judgement state. Preserve stable expected-note identifiers, loop-iteration identity (including repeated notes), noncontiguous bar selections, pause/seek/restart behaviour and the existing hit assessments.
- Use the existing Tone transport frame callback; do not add a parallel timer, animation loop or duplicate subscriptions to make presentation state work.
- The integration is optional and must remain dormant when no renderer is mounted, without unnecessary per-frame presentation work.

### Renderer contract and isolation

- Implement a new renderer against the established lifecycle contract: `initialize`, `update`, `resize` and `dispose`.
- A renderer consumes presentation data and draws it. It must not parse MIDI, decide scoring classifications, modify canonical exercise state or schedule audio.
- Keep SVG-specific (or future Canvas/PixiJS-specific) implementation details inside the renderer, not in the domain, controller or shared playback logic.
- Mount/unmount must clean up subscriptions, animation resources and listeners. Renderer errors must not interrupt playback or scoring.
- The existing VexFlow notation renderer stays on its established path. Do not route it through the new presentation controller or refactor it solely for architectural symmetry without an explicit requirement and tests.
- The controller integration exists, but a user-facing SVG highway and Notation/Highway selector are **not** part of that architectural milestone. Do not assume they are already implemented.

### Presentation-specific verification

When modifying the controller/host or adding a renderer:

- Add mock-renderer tests for controller behaviour and real-app integration tests for transport and feedback.
- Cover noncontiguous selections, loop iterations, repeated-note feedback, pausing, seeking, restarting, renderer disposal and error isolation as applicable.
- Verify the existing notation path remains unchanged, and measure actual renderer performance separately from controller or notation performance.
- Do not claim simulated Web MIDI or headless browser tests establish physical hardware latency or real-device frame rates.

## Setup and focused checks

- Use Node 22, stable Rust with `wasm32-unknown-unknown`, Trunk 0.22.1 and `npm ci`; browser tests also require installed Google Chrome and `python3`, and security QA requires `cargo-audit`. See `QA.md` for setup and check scope.
- Focus a Rust test with `cargo test -p rudiment-core --locked <test-name>` (or `-p rudiment-web`). For a focused browser test, first run `npm run build`, then `npx playwright test tests/e2e/<file>.spec.ts -g '<test name>'`. Playwright starts its own Python server for `dist/` on an ephemeral port; it does not test the dev server.
- `npm run qa:browser`, `qa:a11y` and `qa:performance` each build first. Pass `-- --skip-build` only if the current `dist/` was already built in the same job. QA overwrites `quality-results/latest.{json,md}` on each run; detailed logs are in `quality-results/logs/`.
- `npm run hooks:install` enables tracked hooks; pre-push includes security/browser/a11y, with performance opt-in via `RUDIMENT_PRE_PUSH_PERF=1`.

## Quality-assurance workflow

For UI changes, consult `RUDIMENT_STYLE_GUIDE.md`; its screenshot-derived measurements and colours are approximate, so preserve intentional, accessible implementation tokens rather than replacing them solely to match estimates.

After meaningful changes, run the relevant shared QA entry points before describing work as verified:

- Rust-only code: `npm run qa:quick` and `npm run qa:unit`.
- JavaScript or TypeScript: `npm run qa:quick`; add `npm run qa:browser` when runtime behaviour changes.
- UI, interaction, responsive layout, dialogs, or feedback: `npm run qa:browser` and `npm run qa:a11y`.
- MIDI import, timing, playback, scoring, notation, or rendering: relevant Rust/browser correctness tests plus `npm run qa:performance` (the fast performance suite).
- Dependencies or security-sensitive parsing: `npm run qa:security` plus the relevant unit/browser tests.
- Cross-cutting or release-facing work: `npm run qa:full`.

Review failures, fix regressions introduced by the change, and rerun affected checks. Report unavailable tools, environment failures, skipped checks, hosted-CI status, and hardware-only validation explicitly; never claim an unrun check passed. See `QA.md` and `TESTING.md` for prerequisites and manual checks.

## Musical correctness and performance

- Low-latency interaction is essential. For MIDI/keyboard input, transport, scoring, notation, feedback, scrolling, repetition or audio scheduling, preserve note onset/duration, instrument mapping, sticking, timing classifications and thresholds, scores, repeats, playhead, statistics, and audio/visual synchronisation. Never trade correctness for speed.
- Run relevant Rust and browser correctness tests and `npm run perf:fast` when input, playback, scoring or rendering changes (`qa:performance` runs the same fast measurement). Investigate repeatable regressions. Run `npm run perf:full` for significant architecture changes and before releases; run or schedule `npm run perf:endurance` for lifecycle, memory, timer or long-session changes.
- Base performance refactoring on measurements, keep profiling opt-in, and avoid workers, AudioWorklets, canvas or virtualisation without a real-browser trace demonstrating the need. Update deterministic benchmarks when exercise functionality, rhythms, instruments or import behaviour expands; maintain `scripts/performance`, `tests/performance`, baselines, budgets, CI and `PERFORMANCE.md` together.
- Compare on matching browser/OS/architecture and workload where possible, with adequate samples and both relative and absolute regression margins. Do not weaken accessibility/performance gates or treat an incompatible baseline as relative-regression evidence. Simulated Web MIDI does not validate USB, driver, device, speaker or subjective audio latency.

## Automatic Git commit and push workflow

**Default:** After completing a task that modifies project files, validate, commit and push the completed work. An explicit task instruction to stop without committing or to commit locally without pushing **overrides this default**.

### Initial verification

- Before making changes, inspect `git status`, the current branch, its upstream and outgoing commits. Identify unrelated uncommitted work and unpublished commits.
- Inspect the Git hook configuration and verify that pre-commit and pre-push hooks are installed and active; do not assume that tracked hook files are automatically executing.
- Preserve the existing validation infrastructure. Do not bypass, disable or modify hooks to make a task pass.

### Validation and commit

1. Run the relevant tests, linting, type checks and build validation appropriate to the change. Avoid redundant checks already covered by active hooks unless needed to diagnose a failure.
2. If validation fails, investigate and fix regressions introduced by the task, then rerun affected checks. If a failure is demonstrably pre-existing or unrelated, report it and obtain approval before undertaking substantial out-of-scope fixes; do not claim successful validation or bypass a required failing gate.
3. Review `git status` and the diff. Stage only files belonging to the completed task; never include unrelated changes or secrets.
4. Create a descriptive Conventional Commit (`feat:`, `fix:`, `refactor:`, etc.) and allow pre-commit hooks to run normally.
5. If a commit hook fails, diagnose and fix the cause, then retry the commit with hooks enabled. Do not repeatedly retry an unchanged failure.
6. Do not create empty commits, rewrite published history or commit another agent's changes.

### Push (unless explicitly prohibited)

1. Inspect the commits that would be published to the configured upstream. **If the push would also publish unrelated, previously unpublished commits, disclose them and obtain confirmation before pushing.**
2. Push only to the expected current branch and configured upstream; never force-push or push to an unexpected branch.
3. Allow the pre-push hooks to execute normally. If a hook fails, diagnose and fix its cause, commit any required task-related corrections with hooks enabled, and retry. Do not bypass hooks or repeatedly retry the same unchanged failure.
4. If a push cannot complete safely, preserve the local commit and report the blocker. Never claim remote success until the push has actually succeeded.

### Completion report

Report:

- Summary of changes and relevant files.
- Tests and validation actually run, including failures, fixes, skips and limitations.
- Final commit hash and branch (or why no commit was made).
- Push result, clearly distinguishing **pushed**, **local only by request**, and **blocked/not attempted**.

When multiple agents work concurrently, do not stage, commit or push another agent's changes.
