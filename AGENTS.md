# Rudiment agent instructions

## Architecture and generated assets

- `crates/core/src/lib.rs` is the web-independent MIDI/timing/scoring domain; `web/src/main.rs` is the Leptos/WASM UI and practice state. `web/js/bridge.js` handles Web MIDI, Tone.js audio, VexFlow notation and IndexedDB. Tone.Transport is the musical clock; rendering must not become a second scheduler.
- Edit `web/js/bridge.js`, not the generated `web/public/bridge.js`. `npm run build` regenerates fixtures and bundles the bridge before Trunk writes `dist/`; `npm run dev` does the same and watches sources into `web/dist/` (port 5173, override with `RUDIMENT_PORT`).
- `scripts/generate-fixtures.mjs` generates ignored MIDI files and the performance manifest; the built-in `web/public/exercises/rudiments/*.mid` are tracked. Their test-only reference engravings and manifest are in `tests/fixtures/rudiment-reference/`; keep them out of the deployed app.
- GitHub Pages builds with `--public-url "./"`; preserve relative asset paths so the app also works under a repository subpath.

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
