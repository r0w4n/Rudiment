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
grep -q "startsWith(github.event.comment.body, '/ai revise')" "$caller"
grep -q "task_mode:.*revision" "$caller"
grep -q 'pr_number:' "$caller"
grep -q 'comment_id:' "$caller"
grep -q 'automation_pr_author: central-ai-publisher\[bot\]' "$caller"
grep -q 'approved_base_branch: main' "$caller"
grep -q 'options: \[implementation, revision\]' "$caller"
grep -q 'APP_ID: \${{ secrets.APP_ID }}' "$caller"
grep -q 'APP_PRIVATE_KEY: \${{ secrets.APP_PRIVATE_KEY }}' "$caller"

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

echo 'PASS: generic AI implementation and revision events use immutable Central orchestration; Rudiment owns setup, checks and preview regeneration'
