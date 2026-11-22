#!/usr/bin/env bash
#
# Static security and regression checks for the OpenCode AI task workflow.
#
# These complement the behavioural tests (test-authorise.sh,
# test-verify-result.sh) by asserting invariants of the committed workflow and
# gate scripts: credential isolation, authorisation gating, draft-only pull
# requests, protected-path enforcement and the regression for the Markdown
# code-fence shell bug.
# Usage: bash scripts/opencode-ai-task/test-workflow.sh

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
workflow="$root/.github/workflows/opencode-ai-task.yml"
verify="$here/verify-result.sh"

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

step_lines() { # echoes "start end" of the named step's run block in the workflow
  local name="$1" start end total
  start="$(grep -nE -- "^      - name: ${name}\$" "$workflow" | head -1 | cut -d: -f1)"
  if [ -z "$start" ]; then echo "0 0"; return; fi
  total="$(wc -l <"$workflow")"
  end="$(awk -v s="$((start + 1))" 'NR >= s && /^      - name:/{print NR; exit}' "$workflow")"
  [ -n "$end" ] || end="$total"
  echo "$start $end"
}

# --- Markdown code-fence regression -------------------------------------------
# A bare code-fence line is a shell command and fails the step under set -e.
FENCE='```'
if grep -qE -- "^[[:space:]]*'${FENCE}'[[:space:]]*\$" "$workflow" "$verify"; then
  ko "no bare code-fence commands (bare fence line found)"
else
  ok "no bare code-fence commands"
fi
if grep -q -- "echo '${FENCE}'" "$verify"; then
  ok "code fences rendered with echo"
else
  ko "code fences rendered with echo"
fi

# --- Authorisation gating ------------------------------------------------------
assert_in "$workflow" "implement job gated on authorisation output" \
  "needs\.authorise\.outputs\.proceed == 'true'"
assert_in "$workflow" "issue author gate present" \
  "github\.event\.issue\.user\.login"
assert_in "$workflow" "label application sender gate present" \
  "github\.event\.sender\.login"
[ "$(grep -c "scripts/opencode-ai-task/authorise.sh" "$workflow")" -ge 2 ] &&
  ok "authorisation gate script used in both jobs" ||
  ko "authorisation gate script used in both jobs"

# --- Credential isolation ------------------------------------------------------
# Checkouts must not persist credentials into .git/config, so the model process
# cannot read a GitHub token.
[ "$(grep -c "persist-credentials: false" "$workflow")" -ge 2 ] &&
  ok "checkouts do not persist credentials" ||
  ko "checkouts do not persist credentials"

# The OpenCode implementation step must not receive the GitHub token.
start_end=$(step_lines "Implement the task")
start=${start_end%% *}
end=${start_end##* }
if [ "$start" -gt 0 ] && [ "$end" -gt "$start" ]; then
  if sed -n "${start},${end}p" "$workflow" | grep -qE -- "GH_TOKEN|github\.token|GITHUB_TOKEN"; then
    ko "OpenCode step receives no GitHub token"
  else
    ok "OpenCode step receives no GitHub token"
  fi
else
  ko "OpenCode step located for token check"
fi

# The API key is only referenced where needed (implement + diagnostics).
if [ "$(grep -c -- "OPENCODE_API_KEY:" "$workflow")" -le 2 ]; then
  ok "API key referenced only in scoped steps"
else
  ko "API key referenced only in scoped steps"
fi

# --- Never merge, never force, never deploy -----------------------------------
assert_not_in "$workflow" "no automatic merge" "gh pr merge"
assert_not_in "$workflow" "no marking PR ready for review" "gh pr ready"
assert_not_in "$workflow" "no admin bypass flags" --admin
assert_not_in "$workflow" "no force pushes" "git push[^|]*--force"
assert_in "$workflow" "pull requests are drafts" "gh pr create --draft"
assert_not_in "$workflow" "no pushes to the default branch" \
  'git push origin "\$\{DEFAULT_BRANCH\}'
assert_in "$verify" "guard refuses task branch equal to default branch" \
  "must never be the default branch"

# --- Protected paths -----------------------------------------------------------
assert_in "$verify" "guard blocks .github/ and .githooks/ changes" \
  '\.github/ \.githooks/'
assert_in "$workflow" "diagnostic artifacts scrub the API key" \
  "REDACTED-OPENCODE-KEY"

# --- Scripts exist and are executable -----------------------------------------
for s in authorise.sh verify-result.sh; do
  if [ -x "$here/$s" ]; then ok "gate script executable: $s"; else ko "gate script executable: $s"; fi
done

echo
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
