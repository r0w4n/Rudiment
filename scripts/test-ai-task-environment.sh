#!/usr/bin/env bash

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
caller="$root/.github/workflows/opencode-ai-task.yml"
ai_setup="$root/.github/actions/setup-ai-task/action.yml"
project_setup="$root/.github/actions/setup-rudiment/action.yml"

grep -q 'setup: true' "$caller"
grep -Eq 'opencode-ai-task\.yml@[0-9a-f]{40}$' "$caller"
! grep -q 'toolchain_profile' "$caller"

grep -q "save-cache: 'false'" "$ai_setup"
grep -q 'uses: \./\.github/actions/setup-rudiment' "$ai_setup"
! grep -Eq 'npm run build|trunk build|cargo build' "$ai_setup"

grep -q 'node-version: 22' "$project_setup"
grep -q 'wasm32-unknown-unknown' "$project_setup"
grep -q 'TRUNK_VERSION: 0.22.1' "$project_setup"
grep -q 'save-if: \${{ inputs.save-cache }}' "$project_setup"
grep -q 'channel = "1.99.1"' "$root/rust-toolchain.toml"

echo 'PASS: AI task uses the generic immutable interface without pre-agent compilation or cache saves'
