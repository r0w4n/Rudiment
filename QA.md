# Rudiment quality assurance

Rudiment uses one set of repository commands for developers, Codex, Git hooks, and GitHub Actions. The commands call established tools directly and write a structured report to `quality-results/latest.json`, a readable summary to `quality-results/latest.md`, and full output to `quality-results/logs/`.

## Prerequisites

Use Node.js 22, current stable Rust, Chrome, the `wasm32-unknown-unknown` Rust target, Trunk 0.22.1, and locked npm dependencies. Dependency security also requires `cargo-audit`:

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk --version 0.22.1 --locked
cargo install cargo-audit --locked
npm ci
```

## Shared commands

| Command                  | Scope                                                                                    |
| ------------------------ | ---------------------------------------------------------------------------------------- |
| `npm run qa:quick`       | ESLint, adopted Prettier scope, Rust formatting/checking, and Clippy                     |
| `npm run qa:unit`        | Locked Rust workspace unit and integration tests                                         |
| `npm run qa:security`    | High/critical npm advisories and RustSec advisories                                      |
| `npm run qa:a11y`        | Production build plus focused Playwright/axe WCAG automation                             |
| `npm run qa:browser`     | Production build plus functional and performance-correctness browser tests               |
| `npm run qa:performance` | Production build plus the existing fast real-Chrome measurements and enforced validation |
| `npm run qa:full`        | All mandatory local checks, including fast performance validation                        |
| `npm run qa:report`      | Reprint the latest consolidated result and preserve its failure exit code                |

Each command is unattended and returns non-zero when a mandatory check fails, a required metric is missing, a dependent check is skipped, or a required executable is unavailable. The runner continues independent checks so one invocation yields a useful consolidated report. `--skip-build` may be passed after `--` only when the caller has already produced the current `dist/` in the same job.

Browser and accessibility tests start their own production-build server on an OS-assigned loopback port. They do not reuse an existing server, so a development server or stale process cannot cause a port conflict or make the checks exercise the wrong build.

Prettier initially covers the QA/configuration surface and new accessibility suite. Existing condensed application and long browser-test sources remain linted but are not mechanically reformatted, avoiding a high-risk, unrelated rewrite of timing-sensitive code. New files should join the formatter scope when practical.

## Hooks

Install the tracked hooks once per checkout:

```bash
npm run hooks:install
```

Pre-commit selects shared formatting/static checks from staged file types. It never rewrites staged files. Pre-push runs the broader shared gate: quick checks, unit tests, build, dependency audits, browser tests, and accessibility tests. Set `RUDIMENT_PRE_PUSH_PERF=1` to add the fast performance suite; performance remains opt-in locally because real-browser measurements are slower and environment-sensitive. Hooks are advisory and can be bypassed, so CI repeats mandatory checks independently.

## Security model

- `npm audit --audit-level=high` and `cargo audit` provide local and CI software-composition analysis.
- CodeQL scans JavaScript/TypeScript and Rust on pull requests, `main`, and a schedule. Hosted CodeQL availability and the repository's code-scanning entitlement can only be confirmed after the workflow runs.
- MIDI parsing rejects files over 8 MiB, more than 100,000 events, invalid/truncated SMF data, unsupported timing, invalid tempo/meter metadata, and files without playable notes. Deterministic tests exercise these boundaries, including a pathological event-count fixture.
- Expensive coverage-guided fuzzing is not part of routine checks. It remains a future hardening option if parser changes or threat evidence justify the maintenance cost.
- OWASP ZAP is not a mandatory gate. Rudiment is a static, client-side Pages application with no application server/API, and a baseline scan of Python's local static server would mainly report non-production response headers. Browser input tests, dependency review, CodeQL, CSP/hosting review, and MIDI parser boundaries provide more relevant coverage. Reassess ZAP if network-facing application endpoints are introduced.

## Accessibility scope and limits

Playwright and `@axe-core/playwright` scan the main practice workspace, text-equivalent note feedback, library/settings/statistics surfaces, narrow layout, keyboard focus, and 200% browser zoom against applicable WCAG 2.x A/AA rules. Functional browser tests also cover dialog focus trapping/restoration, responsive layouts, keyboard operation, and visible text alternatives for timing judgements.

Automation does not prove WCAG 2.2 AA conformance. Releases still need manual checks for logical reading/focus order, screen-reader announcements during practice, musical notation comprehension, zoom/reflow across supported devices, non-colour comprehension, audio alternatives, target size, and real keyboard/MIDI workflows. The timing legend and per-note accessible labels provide a non-colour alternative for score feedback, but should be included in manual assistive-technology review.

## Performance authority

The fast suite enforces required metrics, sample minima, matching scenario workloads, finite results, and absolute callback/score-update budgets. Relative comparisons require the same platform, architecture, browser major version, and workload signature. The checked-in baseline is currently macOS/ARM and predates structured browser-version metadata, so Linux CI reports relative changes as diagnostic and enforces only the reliable absolute budgets. Heap growth and animation-frame observations remain diagnostic because Chromium heap sampling is not controlled GC telemetry and headless frame delivery is noisy. A reviewed Linux/x64 baseline generated by the same workflow is required before CI-relative gates can become authoritative.

## GitHub Actions and reports

- `quality.yml`: pull-request and nightly code, unit, build, browser, and accessibility gates.
- `security.yml`: lockfile-triggered and weekly dependency audits.
- `codeql.yml`: hosted JavaScript/TypeScript and Rust SAST.
- `performance.yml`: pull-request fast gates and scheduled/on-demand full plus endurance profiling.
- `pages.yml`: repeats release gates, builds the Pages-relative asset graph once, and deploys only from its successful build job.

Jobs use the local setup action and the same npm entry points as developers. Reports, Playwright failures, performance JSON/Markdown, and traces are retained as workflow artifacts. GitHub's native checks and notifications identify failed jobs; no third-party notification service is required.
