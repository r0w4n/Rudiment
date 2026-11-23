# PR #9 stabilisation — implementation checklist

Task: complete and harden PR #9 (OpenCode AI task automation). Final
stabilisation before future extraction into a reusable automation repository.
Do NOT merge; do not centralise; do not add model routing.

## Checklist

### 1. Publication workflow fix
- [ ] `publish-pr.sh`: trusted publication sequence as a testable script
      (metadata validation → PR body from validated artifact → askpass push →
      draft PR → orphaned-branch cleanup on PR failure).
- [ ] Publish step reads meta/summary/title from the validated artifact
      directory, never from the publication working clone.
- [ ] Invalid/missing metadata fails closed BEFORE any push.
- [ ] PR-creation failure deletes the pushed branch and reports recovery path.
- [ ] Regression tests: valid artifact → correct branch + draft PR with
      correct model/title/summary; missing/malformed metadata rejected;
      failed PR creation leaves no orphaned branch.

### 2. Failure notification fix
- [ ] Job results passed via environment variables, not `${needs.*}` in Bash.
- [ ] `notify-failure.sh` extracted; comment content covered by tests
      (implement/validate/publish failure; success produces no stage lines).
- [ ] Notification step marked continue-on-error so a notification failure
      cannot conceal the original failure.

### 3. Diagnostic redaction hardening
- [ ] `redact.sh` fails closed (no `|| true`), verifies its own output.
- [ ] API key handled literally (awk index replacement) — safe for keys with
      regex metacharacters.
- [ ] Workflow uploads diagnostics only when redaction succeeded
      (step outcome gated).
- [ ] Minimal diagnostics: redacted log tail (200 lines) instead of raw logs.
- [ ] Regression tests: metacharacter keys, silent-failure detection,
      unset-key safety, residual-secret detection.

### 4. Test suites
- [ ] test-publish-safety.sh extended (publication cases + e2e via
      publish-pr.sh).
- [ ] test-notify-failure.sh new.
- [ ] test-redact.sh extended.
- [ ] test-workflow.sh extended (publish-pr.sh call, no `${needs.*}` in run
      blocks, env-passed results, redaction gating, continue-on-error).

### 5. Validation
- [ ] All suites green (authorise 10, export 11, validate 13, publish+,
      redact+, notify+, workflow 48+).
- [ ] `bash -n` all scripts and run blocks; `actionlint` clean;
      `npm run qa:quick`; pre-commit hook.
- [ ] Push; PR #9 description updated; GitHub PR checks observed.

### Status notes
- Progress notes and test results are appended below as work completes.
- Branch: `ci/issue-ai-task-automation` (worktree
  `../rudiment-worktrees/opencode-issue-automation`).
