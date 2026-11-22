#!/usr/bin/env bash
#
# Mocked-payload tests for scripts/opencode-ai-task/authorise.sh.
#
# Runs the authorisation gate against simulated GitHub event data and verifies
# each policy scenario without any network access, credentials or model calls.
# Usage: bash scripts/opencode-ai-task/test-authorise.sh

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
gate="$here/authorise.sh"

pass=0
fail=0

run_gate() {
  # $1 = expected outcome ("allow" | "deny"); remaining env passed through.
  local expected="$1"
  local out rc
  out="$(bash "$gate" 2>/dev/null)" && rc=0 || rc=$?
  if [ "$expected" = "allow" ]; then
    if [ "$rc" -eq 0 ] && grep -q '^proceed=true' <<<"$out"; then
      pass=$((pass + 1))
      echo "PASS: $scenario"
    else
      fail=$((fail + 1))
      echo "FAIL: $scenario (expected allow, got rc=$rc, out=$out)"
    fi
  else
    if [ "$rc" -eq 0 ] && grep -q '^proceed=false' <<<"$out"; then
      pass=$((pass + 1))
      echo "PASS: $scenario"
    else
      fail=$((fail + 1))
      echo "FAIL: $scenario (expected deny, got rc=$rc, out=$out)"
    fi
  fi
}

scenario="authorised: r0w4n created issue and applied ai-task"
GATE_EVENT_LABEL=ai-task GATE_ISSUE_STATE=open GATE_ISSUE_AUTHOR=r0w4n \
GATE_SENDER=r0w4n GATE_IS_PULL_REQUEST=false GATE_BODY_NONEMPTY=true \
GATE_REQUIRED_AUTHOR=r0w4n GATE_REQUIRED_LABEL=ai-task \
run_gate allow

scenario="rejected: issue created by another user"
GATE_EVENT_LABEL=ai-task GATE_ISSUE_STATE=open GATE_ISSUE_AUTHOR=someone-else \
GATE_SENDER=r0w4n GATE_IS_PULL_REQUEST=false GATE_BODY_NONEMPTY=true \
GATE_REQUIRED_AUTHOR=r0w4n GATE_REQUIRED_LABEL=ai-task \
run_gate deny

scenario="rejected: label applied by another user"
GATE_EVENT_LABEL=ai-task GATE_ISSUE_STATE=open GATE_ISSUE_AUTHOR=r0w4n \
GATE_SENDER=someone-else GATE_IS_PULL_REQUEST=false GATE_BODY_NONEMPTY=true \
GATE_REQUIRED_AUTHOR=r0w4n GATE_REQUIRED_LABEL=ai-task \
run_gate deny

scenario="rejected: different label applied"
GATE_EVENT_LABEL=enhancement GATE_ISSUE_STATE=open GATE_ISSUE_AUTHOR=r0w4n \
GATE_SENDER=r0w4n GATE_IS_PULL_REQUEST=false GATE_BODY_NONEMPTY=true \
GATE_REQUIRED_AUTHOR=r0w4n GATE_REQUIRED_LABEL=ai-task \
run_gate deny

scenario="rejected: closed issue"
GATE_EVENT_LABEL=ai-task GATE_ISSUE_STATE=closed GATE_ISSUE_AUTHOR=r0w4n \
GATE_SENDER=r0w4n GATE_IS_PULL_REQUEST=false GATE_BODY_NONEMPTY=true \
GATE_REQUIRED_AUTHOR=r0w4n GATE_REQUIRED_LABEL=ai-task \
run_gate deny

scenario="rejected: label applied to a pull request"
GATE_EVENT_LABEL=ai-task GATE_ISSUE_STATE=open GATE_ISSUE_AUTHOR=r0w4n \
GATE_SENDER=r0w4n GATE_IS_PULL_REQUEST=true GATE_BODY_NONEMPTY=true \
GATE_REQUIRED_AUTHOR=r0w4n GATE_REQUIRED_LABEL=ai-task \
run_gate deny

scenario="rejected: empty issue description"
GATE_EVENT_LABEL=ai-task GATE_ISSUE_STATE=open GATE_ISSUE_AUTHOR=r0w4n \
GATE_SENDER=r0w4n GATE_IS_PULL_REQUEST=false GATE_BODY_NONEMPTY=false \
GATE_REQUIRED_AUTHOR=r0w4n GATE_REQUIRED_LABEL=ai-task \
run_gate deny

scenario="rejected: missing label (labeler spoof attempt via other route)"
GATE_EVENT_LABEL= GATE_ISSUE_STATE=open GATE_ISSUE_AUTHOR=r0w4n \
GATE_SENDER=r0w4n GATE_IS_PULL_REQUEST=false GATE_BODY_NONEMPTY=true \
GATE_REQUIRED_AUTHOR=r0w4n GATE_REQUIRED_LABEL=ai-task \
run_gate deny

scenario="allowed: case-insensitive login comparison matches GitHub semantics"
GATE_EVENT_LABEL=ai-task GATE_ISSUE_STATE=open GATE_ISSUE_AUTHOR=R0W4N \
GATE_SENDER=r0w4n GATE_IS_PULL_REQUEST=false GATE_BODY_NONEMPTY=true \
GATE_REQUIRED_AUTHOR=r0w4n GATE_REQUIRED_LABEL=ai-task \
run_gate allow

scenario="configuration error surfaces as failure (rc=1)"
if env -u GATE_REQUIRED_AUTHOR bash "$gate" >/dev/null 2>&1; then
  fail=$((fail + 1))
  echo "FAIL: $scenario (expected nonzero exit)"
else
  pass=$((pass + 1))
  echo "PASS: $scenario"
fi

echo
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
