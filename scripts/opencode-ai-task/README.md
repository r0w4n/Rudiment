# OpenCode AI task automation — security design

This document describes the security architecture of
`.github/workflows/opencode-ai-task.yml` and its supporting scripts, the
adversarial regression tests that protect the design, and the manual GitHub
configuration required before activation.

## Threat model

- Issue text (title, body) is untrusted input that may contain prompt
  injections or shell metacharacters.
- The OpenCode agent process is untrusted: it may attempt to modify the
  verification tooling, plant Git hooks or credential helpers, exfiltrate
  credentials, or craft malicious patches.
- GitHub's default `GITHUB_TOKEN` must never be reachable from the agent
  process or from any script that executes agent-controlled content.

## Execution stages

```
authorise ─▶ implement ─▶ validate ─▶ publish
    │             │            │
    │             └ diagnostics │
    └───────────────────────────┴──▶ notify (failure comment)
```

- **A. authorise** (read-only): validates issue author, `ai-task` label,
  labeler identity, issue state and description; blocks duplicates
  (per-issue concurrency group + branch/open-PR checks). Produces
  `proceed`, `branch` (`ai/issue-<n>`) and `base_sha` outputs. Never
  invokes the model.
- **B. implement** (`contents: read`, `issues: read`): checks out the
  pinned base commit, snapshots the trusted automation scripts to
  `$RUNNER_TEMP` *before* the agent runs, then runs a version-pinned
  OpenCode without any GitHub write credential. A trusted script exports
  the committed changes as a restricted artifact: numbered `format-patch`
  files, `meta.json`, `pr-summary.md`, `issue-title.txt` and a
  `SHA256SUMS` manifest. No `.git` metadata, hooks, configuration or
  credentials ever enter the artifact. An authenticated push never happens
  in this job.
- **C. validate** (`contents: read`): fresh runner, fresh checkout of the
  trusted base revision; runs `validate-changes.sh` from the trusted
  checkout (never from the artifact). Verifies artifact integrity,
  applies patches with Git hooks disabled into a disposable clone and
  re-checks the resulting tree: no protected paths (`.github/`,
  `.githooks/`, `scripts/opencode-ai-task/`), no symlinks, no
  submodules/`.gitmodules`, no `.git`-internal paths, no path traversal,
  no merge commits, exact commit count, clean tree. On success it
  re-exports a validated artifact. Every rejection is fail-closed.
- **D. publish** (`contents: write`, `pull-requests: write`): fresh
  runner; downloads the validated artifact, re-applies it with
  `validate-changes.sh` (hooks disabled before any Git operation,
  credential helpers cleared, token supplied via `GIT_ASKPASS` so no
  token ever lands in a remote URL or Git config), revalidates the
  resulting tree, then pushes `ai/issue-<n>` and opens a draft PR.
  Never merges, never force-pushes, never touches `main`.
- **notify** (`issues: write`): comments on the originating issue when a
  downstream stage fails, pointing at run logs and diagnostics.

The agent-modified working tree never leaves the implement job: only the
generated patch artifact does, and each later stage re-derives everything
from a trusted checkout.

## Trust boundaries

1. The authorisation gate runs before any model invocation; unauthorised
   events start no jobs that reference secrets.
2. Scripts executed after the agent has run come from a trusted snapshot
   taken before the agent started, or from a fresh checkout of the base
   revision — an AI-modified script on the task branch cannot replace
   them, and any attempt to commit such a modification is rejected by the
   protected-path validation.
3. Cross-job data transfer is limited to the sanitized artifact
   (patches + metadata + checksums), never a `.git` directory.
4. The OpenCode API key exists only in the implement job's agent step and
   (for redaction) the diagnostics step; every artifact and log is
   scrubbed before upload.

## Regression tests

| Test file | Covers |
| --- | --- |
| `test-authorise.sh` | Authorisation matrix (10 mocked scenarios) |
| `test-dedup.sh` | Duplicate prevention (stubbed `gh`, no network) |
| `test-export-changes.sh` | Artifact export invariants; refuses dirty/merge/empty states |
| `test-validate-changes.sh` | Adversarial artifacts: protected paths, tampered verification script, symlinks, submodules, traversal, `.git` paths, corrupt checksums, malformed patches, base-SHA mismatch; binary changes round-trip |
| `test-publish-safety.sh` | Git hooks never execute with the write token; end-to-end simulated publish with stubbed `gh`; failed validation blocks the push; no push to `main`; no automatic merge |
| `test-redact.sh` | Credential scrubbing for diagnostics |
| `test-workflow.sh` | Static invariants of the committed workflow (permissions, pinning, trust ordering, artifact paths) |

## Manual GitHub configuration

1. Settings → Actions → Workflow permissions → **"Allow GitHub Actions to
   create and approve pull requests"** must be enabled, otherwise the
   publish job cannot open the draft PR.
2. Ruleset "main branch protection": set its ref-name condition to the
   default branch and add *Require a pull request before merging* and
   *Require status checks*. Note that `pull_request` runs created with
   `GITHUB_TOKEN` are held for write-access approval on the PR page; the
   AI PR's checks therefore only run after manual approval — which is also
   what prevents required checks from being bypassed.
3. Optional: repository variable `OPENCODE_AI_TASK_MODEL` to override the
   default model (`opencode-go/glm-5.3-flash`).

## Status / progress checklist

- [x] Design documented (this file).
- [x] `authorise.sh` gate (unchanged, 10/10 tests).
- [x] Stage isolation: authorise / implement / validate / publish / notify jobs.
- [x] `dedup-check.sh` extracted with stubbed-`gh` tests.
- [x] `export-changes.sh` restricted artifact with tests.
- [x] `validate-changes.sh` trusted validation with adversarial tests.
- [x] `apply-validated.sh` + hook/credential isolation with tests.
- [x] Workflow restructured; `actionlint` + `bash -n` + static tests clean.
- [x] OpenCode CLI version pinned; secrets redaction for diagnostics.
- [x] Full test suite green.
- [ ] Post-merge (requires activation): end-to-end live verification
      procedure in this file's "End-to-end validation" section.

## End-to-end validation (post-merge procedure)

1. Merge this PR, then run the `OpenCode Go smoke test` workflow to confirm
   credentials still work.
2. Actions → OpenCode AI task → Run workflow: `issue_number` of a scratch
   issue authored by `r0w4n` and labelled `ai-task`, `dry_run: true`.
   Expect: authorisation checks pass, no model run, no credits consumed.
3. Repeat with `dry_run: false` for the same issue. Expect: implement →
   validate → publish succeed, a draft PR appears from `ai/issue-<n>`,
   its `pull_request` runs are held for approval, and approving them from
   the PR page starts the repository's quality/performance/CodeQL checks.
4. Retrigger the same issue: expect the run to skip as a duplicate.
5. Delete the test branch and close the scratch PR afterwards.
