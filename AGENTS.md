# Rudiment agent instructions

Low-latency musical interaction is a primary non-functional requirement. Changes to MIDI or keyboard input, transport/playback, scoring, notation rendering, score feedback, scrolling, repetition, or audio scheduling must be reviewed for both performance and musical correctness.

- Preserve note onset/duration, instrument mapping, sticking, timing classifications and thresholds, scoring, repeats, playhead position, statistics, and audio/visual synchronisation. Never trade correctness for a faster number.
- Run the relevant Rust and browser correctness tests for performance-sensitive changes. Run `npm run perf:fast` when input, playback, scoring, or rendering changes; investigate repeatable regressions before considering the work complete.
- Use measured evidence before performance refactoring. Keep profiling opt-in and avoid architectural changes such as workers, AudioWorklets, canvas, or virtualisation unless a real-browser trace demonstrates the need.
- Update deterministic benchmark fixtures when exercise functionality, supported rhythms, instruments, or import behaviour expands.
- Maintain `scripts/performance`, `tests/performance`, the checked-in baselines, budgets, CI workflow, and `PERFORMANCE.md` together.
- Run `npm run perf:full` for significant architecture changes and before releases. Run or schedule `npm run perf:endurance` for lifecycle, memory, timer, or long-session changes.
- Compare against the same browser/OS/architecture where possible. Require adequate samples plus both relative and absolute regression margins; do not weaken budgets to make a regression pass.
- Record commands that could not run and why. Never represent simulated Web MIDI as evidence of USB, driver, device, speaker, or subjective audio latency.

The repository may contain unrelated user changes. Preserve them and consult `PERFORMANCE.md`, `TESTING.md`, and the existing build conventions before editing.

## Quality-assurance workflow

After meaningful changes, identify the affected areas and run the corresponding shared QA entry point before describing the work as verified:

- Rust-only code: `npm run qa:quick` and `npm run qa:unit`.
- JavaScript or TypeScript: `npm run qa:quick`; add `npm run qa:browser` when runtime behaviour changes.
- UI, interaction, responsive layout, dialogs, or feedback: `npm run qa:browser` and `npm run qa:a11y`.
- MIDI import, timing, playback, scoring, notation, or rendering: relevant Rust/browser correctness tests plus `npm run qa:performance`.
- Dependencies or security-sensitive parsing: `npm run qa:security` plus the relevant unit/browser tests.
- Cross-cutting or release-facing work: `npm run qa:full`.

Review failures, fix failures introduced by the change, and rerun the affected checks. `quality-results/latest.json`, `quality-results/latest.md`, and `quality-results/logs/` contain concise and detailed evidence. Never report a check as passed if it did not run; report unavailable tools, environment failures, skipped checks, hosted-CI status, and hardware-only validation explicitly.

Use the narrower commands during iteration. Do not substitute reasoning for deterministic checks, weaken accessibility or performance gates to obtain a pass, or treat an incompatible performance baseline as relative-regression evidence. See `QA.md` for command scope and prerequisites.
