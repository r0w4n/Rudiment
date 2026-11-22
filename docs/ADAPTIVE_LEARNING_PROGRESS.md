# Adaptive Learning — implementation progress

**Feature branch:** `feat/adaptive-learning`
**Worktree:** `/Users/rowan/Documents/github/Rudiment-worktrees/adaptive-learning`
**Base:** `origin/main` (`27c0285`) — local `main` has unrelated unpublished playhead commits and is intentionally not absorbed.
**Last updated:** 2026-11-22 (Stage 1 checkpoint)

## Overall status

**In progress.** Stage 1 is implemented and verified. Stages 2–6 are not yet implemented.

The repository, committed code, tests and this document are the authoritative record of progress. Do not rely on prior conversation context when resuming.

## Stage checklist

| Stage | Status | Acceptance condition |
|---|---|---|
| 1 — Architecture and controller | **Complete** | Deterministic controller exists in `rudiment-core`, exposes phases/actions/decisions, has unit tests for tempo, recovery, thresholds, bar detection, focus, probes, completion and settings normalisation. |
| 2 — Tempo adaptation | Not started | Controller is connected to playback; aggressive initial recovery, increases, repeats, secondary slowdown and tempo-map scaling are verified. |
| 3 — Difficult-bar detection and focused practice | Not started | Bar-level judgements select a focus passage; playback uses existing bar selection/looping; two successes restore the full selection. |
| 4 — Settings and UI | Not started | Adaptive mode control, dedicated Settings section, persistence, restore defaults and status display are implemented and accessible. |
| 5 — Full-speed probes and completion | Not started | Optional probes, completion gate, restart/stop/interruption and Performance isolation are verified. |
| 6 — Integration and regression | Not started | Relevant QA suites pass; documentation and acceptance criteria verified. |

## Files created or modified

### Stage 1

- `crates/core/src/adaptive.rs` — new controller, settings, phases, actions, decisions and 17 unit tests.
- `crates/core/src/lib.rs` — added `pub mod adaptive;`.
- `docs/ADAPTIVE_LEARNING_PROGRESS.md` — this document.

## Architectural decisions and rationale

1. **Controller lives in `rudiment-core`.** It is web-independent and deterministic. It receives finalised `SessionSummary` results plus the performed bars and current tempo, and returns the next tempo, bars, action and explanation. No playback code is imported.
2. **Star ratings reuse `performance_rating`.** Adaptive decisions therefore stay aligned with existing scoring and achievements without changing note matching or tolerances.
3. **Original active selection is expanded before creating the controller.** The web layer will pass concrete bar indices (from `BarSequence`) so the controller can clamp neighbours and respect noncontiguous user selections.
4. **Expected-note counts per bar are supplied to the controller.** This lets problem-bar detection normalise by bar density instead of always choosing the busiest bar.
5. **Problem-bar detection uses two qualifications.** A bar qualifies when it accounts for at least half of unsuccessful events by raw count *or* by expected-note-normalised share; candidates are then ranked by error rate. This satisfies the “half the errors” rule while avoiding unfair dense-bar bias.
6. **The current focus is retained while it still qualifies.** This implements the requirement to prefer the current focus over unnecessary switching.
7. **Aggressive recovery occurs only on the initial 0–1-star assessment.** Secondary reductions happen only after the configured failed-attempt threshold and use the configured percentage.
8. **Probes are triggered by successful tempo increases.** The controller remembers the pre-probe recovery tempo so an unsuccessful probe returns there without an extra penalty.
9. **Completion is gated on the full original selection at the original target tempo with three stars.** Intermediate adaptive attempts are never completion.
10. **Settings are normalised to documented ranges.** Raw persisted values can be forgiving; the controller always receives deterministic clamped values.

## Tests executed and results

### Stage 1

Command:

```sh
cargo test -p rudiment-core --locked adaptive::
```

Result: **17 passed, 0 failed** (44 pre-existing core tests filtered out).

Coverage includes:

- initial recovery examples (160→80, 120→60, 100→50, 80→50);
- original tempo below minimum;
- 3-star completion, 2-star repeat, 0/1-star recovery;
- failed-attempt threshold and 10% secondary reduction;
- increase step and target cap;
- minimum tempo floor;
- problem-bar detection, neighbour selection, user-subset clamping and distributed errors;
- dense-bar fairness;
- two focused successes restoring the full selection;
- full-speed probe and return to previous tempo;
- completion requiring the full selection at target;
- settings range normalisation;
- reset behaviour.

## Known issues, blockers and outstanding bugs

- No playback integration yet; the controller is not reachable from the UI.
- No persistence or UI yet.
- No adaptive attempt/history tagging yet.
- The final merge/push is blocked by unrelated unpublished commits on local `main` (see below). Do not push them without explicit approval.
- Full browser/performance/a11y validation has not been run yet.

## Git and checkpoint history

| Checkpoint | Commit | Notes |
|---|---|---|
| Stage 1 | _pending at document creation_ | Controller + tests + progress document. |

## Immediate next actions

1. Commit Stage 1.
2. Stage 2: add adaptive state to `web/src/main.rs`; connect `start_new_run`, attempt completion and `finish_run` boundaries; apply controller tempo decisions; force Practice mode and loops=1; verify tempo-map scaling with tests.
3. Continue with Stage 3 focus integration, Stage 4 settings/UI, Stage 5 probes/lifecycle, Stage 6 QA.

## How to resume in a new agent session

1. Read `AGENTS.md`.
2. Read this document.
3. Run `git status`, `git worktree list`, `git branch --show-current` and `git log --oneline -10` in the worktree above.
4. Verify the Stage 1 files and tests still exist and pass:
   `cargo test -p rudiment-core --locked adaptive::`.
5. Review the “Immediate next actions” and continue from the first incomplete stage. Do not recreate completed controller code.

## Integration note (important)

Local `main` is ahead of `origin/main` by three unrelated commits:

- `aba26b6 smooth`
- `b6bb274 applying fixes to reduce stutter`
- `cee430f chore: add temporary playhead diagnostic`

This feature branch is based on `origin/main` so those commits are not absorbed. At integration time, do **not** push `main` until the user approves publishing those unrelated commits, or until they are handled separately.
