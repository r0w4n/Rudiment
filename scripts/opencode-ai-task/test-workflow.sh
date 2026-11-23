#!/usr/bin/env bash
#
# Static security and regression checks for the OpenCode AI task workflow.
#
# Complements the behavioural tests by asserting invariants of the committed
# workflow file and gate scripts: staged isolation, credential isolation,
# trusted-script ordering, artifact-only data flow, authorisation gating,
# draft-only pull requests and the Markdown code-fence regression.
# Usage: bash scripts/opencode-ai-task/test-workflow.sh

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
workflow="$root/.github/workflows/opencode-ai-task.yml"

pass=0
fail=0

ok() { pass=$((pass + 1)); echo "PASS: $1"; }
ko() { fail=$((fail + 1)); echo "FAIL: $1"; }

assert_in() { # $1 = file, $2 = scenario, $3 = pattern
  if grep -qE -- "$3" "$1"; then ok "$2"; else ko "$2 (missing: $3)"; fi
}

assert_not_in() { # $1 = file, $2 = scenario, $3 = pattern
  if grep -qE -- "$3" "$1"; then ko "$2 (found: $3)"; else ok "$2"; fi
}

job_block() { # echoes "start end" lines of the named job
  local name="$1" start end total
  start="$(grep -nE "^  ${name}:" "$workflow" | head -1 | cut -d: -f1)"
  if [ -z "$start" ]; then echo "0 0"; return; fi
  total="$(wc -l <"$workflow")"
  end="$(awk -v s="$((start + 1))" 'NR >= s && /^  [a-z]+:/{print NR; exit}' "$workflow")"
  [ -n "$end" ] || end="$total"
  echo "$start $end"
}

step_range() { # echoes "start end" of the named step inside the workflow
  local name="$1" start end total
  start="$(grep -nE -- "^      - name: ${name}\$" "$workflow" | head -1 | cut -d: -f1)"
  if [ -z "$start" ]; then echo "0 0"; return; fi
  total="$(wc -l <"$workflow")"
  end="$(awk -v s="$((start + 1))" 'NR >= s && /^      - name:/{print NR; exit}' "$workflow")"
  [ -n "$end" ] || end="$total"
  echo "$start $end"
}

# --- Stages exist --------------------------------------------------------------
for job in authorise implement validate publish notify; do
  job_block "$job" | grep -qv '^0 0$' && ok "stage job present: $job" \
    || ko "stage job present: $job"
done

# --- Markdown code-fence regression -------------------------------------------
# A bare code-fence line is a shell command and fails the step under set -e.
FENCE='```'
if grep -qE -- "^[[:space:]]*'${FENCE}'[[:space:]]*\$" "$workflow" \
  "$here/export-changes.sh" "$here/validate-changes.sh"; then
  ko "no bare code-fence commands (bare fence line found)"
else
  ok "no bare code-fence commands"
fi

# --- Authorisation gating ------------------------------------------------------
assert_in "$workflow" "implement job gated on authorisation output" \
  "needs\.authorise\.outputs\.proceed == 'true'"
[ "$(grep -c "needs.authorise.outputs.proceed == 'true'" "$workflow")" -ge 3 ] &&
  ok "all model-touching stages gated on authorisation" ||
  ko "all model-touching stages gated on authorisation"
assert_in "$workflow" "issue author gate present" \
  "github\.event\.issue\.user\.login"
assert_in "$workflow" "label application sender gate present" \
  "github\.event\.sender\.login"
[ "$(grep -c "scripts/opencode-ai-task/authorise.sh" "$workflow")" -ge 2 ] &&
  ok "authorisation gate script used in authorise and implement jobs" ||
  ko "authorisation gate script used in authorise and implement jobs"
assert_in "$workflow" "dry-run dispatch mode preserved" \
  "inputs\.dry_run"

# --- Job permissions -----------------------------------------------------------
pb="$(job_block implement)"; p_start=${pb%% *}; p_end=${pb##* }
if sed -n "${p_start},${p_end}p" "$workflow" | grep -q "contents: write"; then
  ko "implement job has no write permissions"
else
  ok "implement job has no write permissions"
fi
vb="$(job_block validate)"; v_start=${vb%% *}; v_end=${vb##* }
if sed -n "${v_start},${v_end}p" "$workflow" | grep -qE "contents: write|pull-requests: write"; then
  ko "validate job has write permissions"
else
  ok "validate job has write permissions"
fi
nb="$(job_block publish)"; n_start=${nb%% *}; n_end=${nb##* }
if sed -n "${n_start},${n_end}p" "$workflow" | grep -q "contents: write"; then
  ok "publish job holds the write permissions"
else
  ko "publish job holds the write permissions"
fi
if sed -n "${n_start},${n_end}p" "$workflow" | grep -q "issues: write"; then
  ko "publish job does not hold issues: write"
else
  ok "publish job does not hold issues: write"
fi

# --- Credential isolation ------------------------------------------------------
[ "$(grep -c "persist-credentials: false" "$workflow")" -ge 3 ] &&
  ok "every checkout drops persisted credentials" ||
  ko "every checkout drops persisted credentials"
[ "$(grep -c "ref: \${{ needs.authorise.outputs.base_sha }}" "$workflow")" -eq 3 ] &&
  ok "implement/validate/publish check out the pinned base revision" ||
  ko "implement/validate/publish check out the pinned base revision"

os="$(step_range "Implement the task")"; o_start=${os%% *}; o_end=${os##* }
if sed -n "${o_start},${o_end}p" "$workflow" | grep -qE "GH_TOKEN|github\.token|GITHUB_TOKEN"; then
  ko "OpenCode step receives no GitHub token"
else
  ok "OpenCode step receives no GitHub token"
fi
if [ "$(grep -c -- "OPENCODE_API_KEY:" "$workflow")" -le 2 ]; then
  ok "API key referenced only in the implement and diagnostics steps"
else
  ko "API key referenced only in the implement and diagnostics steps"
fi

# --- Trusted script ordering ---------------------------------------------------
ss="$(step_range "Snapshot trusted automation scripts")"; s_start=${ss%% *}; s_end=${ss##* }
ie="$(step_range "Implement the task")"; i_start=${ie%% *}
if [ "$s_start" -gt 0 ] && [ "$s_end" -le "$i_start" ]; then
  ok "trusted scripts snapshotted before the agent runs"
else
  ko "trusted scripts snapshotted before the agent runs"
fi
assert_in "$workflow" "export runs from the trusted snapshot" \
  'bash "\$\{TRUSTED_SCRIPTS\}/export-changes.sh"'
assert_in "$workflow" "diagnostics redacted from the trusted snapshot" \
  'bash "\$\{TRUSTED_SCRIPTS\}/redact.sh"'
assert_in "$workflow" "validation runs from the trusted checkout" \
  "bash scripts/opencode-ai-task/validate-changes.sh"

# --- Publication sequence -----------------------------------------------------
assert_in "$workflow" "publication runs the trusted publish script" \
  "bash scripts/opencode-ai-task/publish-pr.sh"
assert_in "$workflow" "publication reads the validated artifact" \
  'ARTIFACT_DIR: \$\{\{ runner.temp \}\}/opencode-validated'
assert_not_in "$workflow" "publication step does not read artifact files from the work clone" \
  'cat issue-title.txt'
assert_in "$here/publish-pr.sh" "publication validates metadata before pushing" \
  "is missing from the validated artifact"
assert_in "$here/publish-pr.sh" "publication cleans up the branch if PR creation fails" \
  "no orphaned branch remains"
assert_not_in "$here/publish-pr.sh" "publish script keeps no token in remote URLs" \
  "x-access-token:.*@github"

# --- Failure notification -----------------------------------------------------
assert_in "$workflow" "notification passes stage results via environment" \
  'IMPLEMENT_RESULT: \$\{\{ needs.implement.result \}\}'
if grep -qE '\$\{needs\.[a-z]+\.result\}' "$workflow"; then
  ko "no GitHub expressions interpolated into Bash (\${needs.*})"
else
  ok "no GitHub expressions interpolated into Bash (\${needs.*})"
fi
assert_in "$workflow" "notification marked continue-on-error" \
  "continue-on-error: true"
assert_in "$workflow" "notification script exists and is executable" \
  "notify-failure.sh"

# --- Redaction is fail closed -------------------------------------------------
assert_not_in "$workflow" "redaction failures are not suppressed" \
  'redact.sh" "\$\{DIAG\}" 2>/dev/null \|\| true'
assert_not_in "$workflow" "redaction failures are not suppressed (bare)" \
  '\|\| true$.*redact|redact.*\|\| true'
assert_in "$workflow" "diagnostics upload gated on successful redaction" \
  "steps.diagnostics.outcome == 'success'"
assert_in "$here/redact.sh" "redaction verifies its own output" \
  "verify_clean"
assert_in "$workflow" "diagnostics use a log tail, not raw agent logs" \
  "opencode-log-tail.txt"

# --- Diagnostic secret safety -------------------------------------------------
diag="$(step_range "Collect and redact failure diagnostics")"; d_start=${diag%% *}; d_end=${diag##* }
if [ "$d_start" -gt 0 ] && sed -n "${d_start},${d_end}p" "$workflow" | grep -q -- "OPENCODE_API_KEY:"; then
  ok "diagnostics step receives the API key for redaction"
else
  ko "diagnostics step receives the API key for redaction"
fi
if sed -n "${d_start},${d_end}p" "$workflow" | grep -qE 'diff\.patch|cp -f "\$\{SPEC_FILE\}"|cp -f "\$\{PR_SUMMARY_FILE\}"'; then
  ko "diagnostics are minimal (no raw diff, spec or summary uploads)"
else
  ok "diagnostics are minimal (no raw diff, spec or summary uploads)"
fi
assert_in "$workflow" "diagnostics include only file-name lists" \
  "changed-files.txt"

# --- Dedicated CI regression workflow ----------------------------------------
tests_wf="$root/.github/workflows/opencode-ai-task-tests.yml"
if [ -f "$tests_wf" ]; then
  ok "dedicated security regression workflow exists"
else
  ko "dedicated security regression workflow exists"
fi
if grep -q "pull_request:" "$tests_wf" && grep -q "^on:" "$tests_wf"; then
  ok "regression workflow triggers on pull requests"
else
  ko "regression workflow triggers on pull requests"
fi
assert_in "$tests_wf" "regression workflow runs every test file" \
  'test-\*\.sh'
assert_in "$tests_wf" "regression workflow fails on any test failure" \
  "exit 1"
assert_in "$tests_wf" "regression workflow has read-only permissions" \
  "contents: read"
assert_in "$tests_wf" "regression workflow drops persisted credentials" \
  "persist-credentials: false"

# --- Artifact-only data flow ---------------------------------------------------
assert_in "$workflow" "implement uploads only the artifact directory" \
  'name: opencode-implementation'
assert_in "$workflow" "validate uploads only the validated artifact" \
  'name: opencode-validated'
assert_not_in "$workflow" "no job uploads repository or .git content" \
  "path: \\\$\{\{ github.workspace"
assert_in "$here/lib.sh" "artifact integrity manifest used" "SHA256SUMS"

# --- Hook and credential hygiene in publish -----------------------------------
assert_in "$here/publish-pr.sh" "publish disables hooks before authenticated ops" \
  'git -C "\$\{PUBLISH_WORK\}" config core.hooksPath'
assert_in "$here/publish-pr.sh" "publish supplies the token via GIT_ASKPASS" \
  'GIT_ASKPASS='
assert_not_in "$here/publish-pr.sh" "no token in remote URLs" \
  "x-access-token:.*@"
assert_in "$workflow" "publish revalidates before pushing" \
  "bash scripts/opencode-ai-task/apply-validated.sh"
assert_in "$here/publish-pr.sh" "push uses an explicit branch refspec" \
  'git -C "\$\{PUBLISH_WORK\}" push --no-verify publish-origin "\$\{BRANCH\}:\$\{BRANCH\}"'
assert_in "$here/publish-pr.sh" "publish clears credential helpers" \
  'config credential.helper ""'

# --- Version pinning -----------------------------------------------------------
assert_in "$workflow" "OpenCode CLI version pinned" \
  "OPENCODE_CLI_VERSION: '2"

# --- Never merge, never force, never deploy -----------------------------------
assert_not_in "$workflow" "no automatic merge" "gh pr merge"
assert_not_in "$workflow" "no marking PR ready for review" "gh pr ready"
assert_not_in "$workflow" "no admin bypass flags" --admin
assert_not_in "$workflow" "no force pushes" "git push[^|]*--force[^v]"
assert_in "$here/publish-pr.sh" "pull requests are drafts" "gh pr create --draft"
assert_not_in "$workflow" "no pushes to the default branch" \
  'git push[^|]*"[[:space:]]*"?\$\{DEFAULT_BRANCH\}'
assert_in "$here/validate-changes.sh" \
  "validation refuses a task branch not matching the issue" \
  "does not match ai/issue"
assert_not_in "$workflow" "no deploy actions" "deploy-pages"

# --- Gate scripts executable ---------------------------------------------------
for s in authorise.sh dedup-check.sh export-changes.sh validate-changes.sh apply-validated.sh redact.sh; do
  if [ -x "$here/$s" ]; then ok "gate script executable: $s"; else ko "gate script executable: $s"; fi
done

echo
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
