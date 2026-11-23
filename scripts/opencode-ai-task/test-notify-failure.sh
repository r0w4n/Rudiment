#!/usr/bin/env bash
#
# Tests for scripts/opencode-ai-task/notify-failure.sh.
#
# The stage results arrive as plain environment variables (as the workflow
# passes them); gh interactions are stubbed. Verifies that each failing stage
# is identified, that success produces no stage lines, and that a failing gh
# call surfaces an error without altering other outcomes.
# Usage: bash scripts/opencode-ai-task/test-notify-failure.sh

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

pass=0
fail=0
tmp="$(mktemp -d /tmp/notify-tests.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT

check() {
  if eval "$2"; then
    pass=$((pass + 1)); echo "PASS: $1"
  else
    fail=$((fail + 1)); echo "FAIL: $1"
  fi
}

run_notify() { # env results via caller; $1 = comment file
  COMMENT_FILE="$1" NOTIFY_DRY_RUN=1 ISSUE_NUMBER=5 BRANCH=ai/issue-5 \
    DEFAULT_BRANCH=main RUN_URL="https://stub.invalid/run/1" \
    bash "$here/notify-failure.sh" >/dev/null 2>"$tmp/stderr"
}

# Implementation failure is named, with the diagnostics artifact reference.
run_notify "$tmp/impl.md"
IMPLEMENT_RESULT=failure run_notify "$tmp/impl.md"
check "implementation failure is reported" \
  "grep -q 'implementation' '$tmp/impl.md' && grep -q 'opencode-task-issue-5-diagnostics' '$tmp/impl.md'"
check "no other stage listed for implementation failure" \
  "! grep -qE '^-( validation| publication)' '$tmp/impl.md'"

# Validation failure.
run_notify "$tmp/val.md"
VALIDATE_RESULT=failure run_notify "$tmp/val.md"
check "validation failure is reported" \
  "grep -q 'validation (the implementation artifact was rejected)' '$tmp/val.md'"

# Publication failure.
run_notify "$tmp/pub.md"
PUBLISH_RESULT=failure run_notify "$tmp/pub.md"
check "publication failure is reported" \
  "grep -q 'publication (branch push or pull request creation)' '$tmp/pub.md'"

# Multiple simultaneous failures are all listed.
IMPLEMENT_RESULT=failure VALIDATE_RESULT=failure PUBLISH_RESULT=failure \
  run_notify "$tmp/all.md"
check "all failing stages are listed" \
  "grep -c '^-' '$tmp/all.md' | grep -q 3"

# Success results produce no stage lines.
IMPLEMENT_RESULT=success VALIDATE_RESULT=success PUBLISH_RESULT=success \
  run_notify "$tmp/ok.md"
check "success produces no stage lines" \
  "grep -q 'none recorded' '$tmp/ok.md' && ! grep -q '^- ' '$tmp/ok.md'"

# Comment always identifies the run and the recovery guidance.
check "comment contains the run URL" \
  "grep -q 'https://stub.invalid/run/1' '$tmp/impl.md'"
check "comment confirms nothing merged or deployed" \
  "grep -q 'Nothing was merged, pushed to .main. or deployed' '$tmp/impl.md'"

# A failing gh call exits non-zero (continue-on-error in the workflow keeps
# the original failure authoritative).
mkdir -p "$tmp/fb"
cat >"$tmp/fb/gh" <<'FAKEGH'
#!/bin/sh
echo "stub gh failure" >&2
exit 1
FAKEGH
chmod +x "$tmp/fb/gh"
if IMPLEMENT_RESULT=failure ISSUE_NUMBER=5 BRANCH=ai/issue-5 DEFAULT_BRANCH=main \
  REPOSITORY=r0w4n/Rudiment PATH="$tmp/fb:$PATH" \
  COMMENT_FILE="$tmp/ghfail.md" bash "$here/notify-failure.sh" \
  >/dev/null 2>"$tmp/ghfail.err"; then
  GHFAIL_RC=0
else
  GHFAIL_RC=$?
fi
check "failing gh call surfaces an error" "[ '$GHFAIL_RC' -ne 0 ]"
check "comment file still written for later inspection" \
  "[ -f '$tmp/ghfail.md' ]"

# A successful gh call exits zero (stub).
cat >"$tmp/fb/gh" <<'FAKEGH'
#!/bin/sh
exit 0
FAKEGH
chmod +x "$tmp/fb/gh"
IMPLEMENT_RESULT=failure ISSUE_NUMBER=5 BRANCH=ai/issue-5 DEFAULT_BRANCH=main \
  REPOSITORY=r0w4n/Rudiment PATH="$tmp/fb:$PATH" \
  COMMENT_FILE="$tmp/ghok.md" \
  bash "$here/notify-failure.sh" >/dev/null 2>&1 && GHOK_RC=0 || GHOK_RC=$?
check "successful gh call posts the comment" "[ '$GHOK_RC' -eq 0 ]"

echo
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
