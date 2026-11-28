#!/usr/bin/env bash

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
caller="$root/.github/workflows/opencode-ai-task.yml"
ai_setup="$root/.github/actions/setup-ai-task/action.yml"
project_setup="$root/.github/actions/setup-rudiment/action.yml"
preview="$root/.github/workflows/ai-pr-preview.yml"

grep -q 'setup: true' "$caller"
grep -Eq 'opencode-ai-task\.yml@[0-9a-f]{40}$' "$caller"
! grep -q 'toolchain_profile' "$caller"
grep -q 'issue_comment:' "$caller"
grep -q 'types: \[created\]' "$caller"
# All PR comments are routed; Central owns the comment classification.
grep -q "github.event_name == 'issue_comment' && github.event.issue.pull_request" "$caller"
! grep -q "startsWith(github.event.comment.body" "$caller"
grep -q "task_mode:.*revision" "$caller"
grep -q 'pr_number:' "$caller"
grep -q 'comment_id:' "$caller"
grep -q "options: \[implementation, revision, remediation\]" "$caller"
grep -q 'remediation_event:' "$caller"
grep -q 'remediation_max_attempts:' "$caller"
grep -q 'caller_workflow: opencode-ai-task\.yml' "$caller"
grep -Eq 'actions: write' "$caller"
# Failing-check verification goes through the Checks API ('checks: read' is a
# separate GITHUB_TOKEN scope; a called workflow can never elevate).
grep -Eq 'checks: read' "$caller"
grep -q 'automation_pr_author: central-ai-publisher\[bot\]' "$caller"
grep -q 'approved_base_branch: main' "$caller"
grep -q 'APP_ID: \${{ secrets.APP_ID }}' "$caller"
grep -q 'APP_PRIVATE_KEY: \${{ secrets.APP_PRIVATE_KEY }}' "$caller"

# The CI remediation router only routes events; Central owns remediation
# policy, state, agent execution and publication.
router="$root/.github/workflows/ai-ci-remediation.yml"
grep -q 'workflow_run:' "$router"
for workflow in 'Quality assurance' 'Performance regression checks' 'CodeQL static security analysis' 'Dependency security'; do
  grep -qF -- "- $workflow" "$router"
done
grep -q 'types: \[completed\]' "$router"
# timed_out and startup_failure are actionable conclusions too; per-check
# conclusions are filtered in the evaluate script.
grep -q "conclusion == 'timed_out'" "$router"
grep -q "conclusion == 'startup_failure'" "$router"
grep -qF 'AUTOMATION_PR_AUTHOR: central-ai-publisher[bot]' "$router"
# The pulls-list 'head' filter expects '<owner>:<branch>' (owner only), and
# gh encodes the query parameters.
grep -qF '${REPOSITORY%%/*}' "$router"
grep -qF 'head="${owner}:${branch}"' "$router"
grep -qF 'task_mode: $mode' "$router"
grep -qF 'remediation_event: $event' "$router"
grep -qF 'remediation_run_id: $run_id' "$router"
grep -qF 'actions/workflows/${AI_TASK_WORKFLOW}/dispatches' "$router"
grep -qF 'remediation_event' "$router"
grep -Eq 'checks: read' "$router"
# The router never runs PR code, pushes branches, or merges.
! grep -Eq 'git push|gh pr merge|gh pr (review|checkout)' "$router"
! grep -Eq 'runs-on.*\$\{|uses: [^.]' "$router"

# Behavioural router tests run the extracted evaluate script against
# realistic GitHub event payloads and API responses.
bash "$root/scripts/test-ai-ci-remediation-router.sh"

grep -q "save-cache: 'false'" "$ai_setup"
grep -q 'uses: \./\.github/actions/setup-rudiment' "$ai_setup"
! grep -Eq 'npm run build|trunk build|cargo build' "$ai_setup"

grep -q 'node-version: 22' "$project_setup"
grep -q 'wasm32-unknown-unknown' "$project_setup"
grep -q 'TRUNK_VERSION: 0.22.1' "$project_setup"
grep -q 'save-if: \${{ inputs.save-cache }}' "$project_setup"
grep -q 'channel = "1.99.1"' "$root/rust-toolchain.toml"

# An App-authenticated revision push emits pull_request/synchronize, which
# reuses the existing project-owned checks and replaces the same preview URL.
grep -q 'types: \[opened, synchronize, reopened, closed\]' "$preview"
grep -q 'PREVIEW_PATH: pr-\${{ github.event.pull_request.number }}' "$preview"
grep -q 'pull_request:' "$root/.github/workflows/quality.yml"
grep -q 'pull_request:' "$root/.github/workflows/performance.yml"
grep -q 'pull_request:' "$root/.github/workflows/codeql.yml"
grep -q 'pull_request:' "$root/.github/workflows/security.yml"

echo 'PASS: generic AI implementation, revision, remediation routing and feedback use immutable Central orchestration; Rudiment owns setup, checks and preview regeneration'
