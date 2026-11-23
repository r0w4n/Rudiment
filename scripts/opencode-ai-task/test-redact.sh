#!/usr/bin/env bash
#
# Tests for scripts/opencode-ai-task/redact.sh with dummy credentials.
# Usage: bash scripts/opencode-ai-task/test-redact.sh

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

pass=0
fail=0
tmp="$(mktemp -d /tmp/redact-tests.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT

KEY="oc-test-key-AbCdEf123456"
printf 'Authorization: Bearer gho_REALTOKEN0123456789\nkey=%s\nremote=https://x-access-token:secret@github.com/r/r\npat=github_pat_AAAA0123456789AAAA\n' \
  "$KEY" >"$tmp/log.txt"

OPENCODE_API_KEY="$KEY" bash "$here/redact.sh" "$tmp" >/dev/null
CONTENT="$(cat "$tmp/log.txt")"

check() {
  if eval "$2"; then
    pass=$((pass + 1)); echo "PASS: $1"
  else
    fail=$((fail + 1)); echo "FAIL: $1"
  fi
}

check "API key value redacted" \
  "! printf '%s' \"\$(cat '$tmp/log.txt')\" | grep -qF '$KEY'"
check "API key replaced with marker" \
  "grep -qF '[REDACTED-OPENCODE-KEY]' '$tmp/log.txt'"
check "bearer header redacted" \
  "! grep -q 'gho_REALTOKEN' '$tmp/log.txt'"
check "x-access-token URL redacted" \
  "! grep -q 'secret@github.com' '$tmp/log.txt'"
check "github_pat_ redacted" \
  "! grep -q 'github_pat_AAAA' '$tmp/log.txt'"

# An unset key must not corrupt files (empty sed pattern regression).
printf 'innocent line\n' >"$tmp/plain.txt"
bash "$here/redact.sh" "$tmp" >/dev/null
check "unset key does not corrupt files" \
  "grep -q 'innocent line' '$tmp/plain.txt' && [ \"\$(wc -l <'$tmp/plain.txt')\" -eq 1 ]"

echo
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
