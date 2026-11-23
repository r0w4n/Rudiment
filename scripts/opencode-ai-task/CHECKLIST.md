# PR #9 stabilisation — implementation checklist

Task: complete and harden PR #9 (OpenCode AI task automation). Final
stabilisation before future extraction into a reusable automation repository.
Do NOT merge; do not centralise; do not add model routing.

### Checklist

### 1. Publication workflow fix
- [x] `publish-pr.sh`: trusted publication sequence as a testable script
      (metadata validation → PR body from validated artifact → askpass push →
      draft PR → orphaned-branch cleanup on PR failure).
- [x] Publish step reads meta/summary/title from the validated artifact
      directory, never from the publication working clone.
- [x] Invalid/missing metadata fails closed BEFORE any push.
- [x] PR-creation failure deletes the pushed branch and reports recovery path.
- [x] Regression tests: valid artifact → correct branch + draft PR with
      correct model/title/summary; missing/malformed metadata rejected;
      failed PR creation leaves no orphaned branch.

### 1b. Diagnostic secret safety (final hardening round)
- [x] Diagnostics step now receives `OPENCODE_API_KEY` so `redact.sh` can
      scrub the key value (previously ran without it).
- [x] Diagnostics minimised to a redacted log tail plus file-name lists
      (`changed-files.txt`, `status.txt`); raw diffs, the issue
      specification and the agent summary are no longer uploaded.
- [x] Upload remains gated on successful redaction
      (`steps.diagnostics.outcome == 'success'`).
- [x] Static tests assert the key is present, raw content is absent.

### 1c. CI regression coverage (final hardening round)
- [x] New `.github/workflows/opencode-ai-task-tests.yml`: runs every
      `scripts/opencode-ai-task/test-*.sh` suite on pull requests
      (paths-filtered) and `workflow_dispatch`; fails if any test fails;
      read-only permissions; credential persistence disabled.
- [x] Paths filter removed after review: the regression workflow now runs
      on EVERY pull request so the check is safe to require for all PRs;
      static test asserts no paths filter; README documents it.
- [x] Static tests cover the new workflow.
- [ ] Manual: add "OpenCode AI task security regression tests" to the
      ruleset's required status checks.

### 2. Failure notification fix
- [x] Job results passed via environment variables, not `${needs.*}` in Bash.
- [x] `notify-failure.sh` extracted; comment content covered by tests
      (implement/validate/publish failure; success produces no stage lines).
- [x] Notification step marked continue-on-error so a notification failure
      cannot conceal the original failure.

### 3. Diagnostic redaction hardening
- [x] `redact.sh` fails closed (no `|| true`), verifies its own output.
- [x] API key handled literally (awk index replacement) — safe for keys with
      regex metacharacters.
- [x] Workflow uploads diagnostics only when redaction succeeded
      (step outcome gated).
- [x] Minimal diagnostics: redacted log tail (200 lines) instead of raw logs.
- [x] Regression tests: metacharacter keys, silent-failure detection,
      unset-key safety, residual-secret detection.

### 4. Test suites
- [x] test-publish-safety.sh extended (publication cases + e2e via
      publish-pr.sh) — 34/34.
- [x] test-notify-failure.sh new — 11/11.
- [x] test-redact.sh extended — 9/9.
- [x] test-workflow.sh extended — 64/64.

### 5. Validation
- [x] All suites green: authorise 10/10, export 11/11, validate 13/13,
      publish-safety 34/34, redact 9/9, notify 11/11, workflow 64/64
      (152 assertions total).
- [x] `bash -n` all scripts and run blocks; `actionlint` clean;
      `npm run qa:quick` 5/5; pre-commit hook passed.
- [x] Pushed (`3113819..566ec01`); PR #9 description updated; GitHub PR
      checks all pass (Code quality 4m56s, Browser correctness/accessibility
      6m54s, CodeQL js+rust, fast performance 3m16s; `extended` skipped as
      designed).

### Test session notes (latest run)
- 2026-11-23: all fixes implemented; suites listed above executed locally
  with disposable repositories, stubbed `gh` and dummy credentials
  (`ghp_dummy...`). No production credentials used, no destructive tests
  against the real repository, no model calls.
- Bugs found by the new tests during development: stale metadata leaked
  between test cases (test fix); `::error` lines go to stdout (test fix);
  awk POSIX class syntax rejected `[: ]` and unescaped `/` in bracket
  expressions (script fix — now uses `( |:)*` and escaped `\/`).


### Status notes
- Progress notes and test results are appended below as work completes.
- Branch: `ci/issue-ai-task-automation` (worktree
  `../rudiment-worktrees/opencode-issue-automation`).
