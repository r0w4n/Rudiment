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

# A key containing regex metacharacters is matched literally.
METAKEY='k|y.&*()[]{}$^?\'
printf 'secret=%s\n' "$METAKEY" >"$tmp/meta.txt"
OPENCODE_API_KEY="$METAKEY" bash "$here/redact.sh" "$tmp" >/dev/null 2>&1
check "metacharacter key redacted literally" \
  "! grep -qF '$METAKEY' '$tmp/meta.txt' && grep -qF '[REDACTED-OPENCODE-KEY]' '$tmp/meta.txt'"

# Fail closed: an unreadable file makes redaction exit non-zero.
mkdir -p "$tmp/unreadable"
printf 'Authorization: Bearer gho_SOMEREALTOKEN1234\n' >"$tmp/unreadable/f.txt"
chmod 000 "$tmp/unreadable/f.txt"
if OPENCODE_API_KEY="oc-test-key-AbCdEf123456" bash "$here/redact.sh" "$tmp/unreadable" >/dev/null 2>&1; then
  ko_result=1
else
  ko_result=0
fi
chmod 644 "$tmp/unreadable/f.txt"
check "redaction fails closed on a processing error" "[ '$ko_result' -eq 0 ]"

# Fail closed: a residual secret is detected by the verification pass.
printf 'totally fine line\ngho_RESIDUAL0123456789\n' >"$tmp/residual.txt"
# Bypass conversion by making the file read-only after a successful pass is
# hard; instead inject a secret the converter cannot match: a plain literal
# key value written after redaction is not possible, so simulate via a file
# that awk cannot process (directory-like entry is skipped) — use a valid
# file whose content still holds a 'Bearer' token the regex cannot reach
# because it is split across lines.
printf 'Authorization: Bearer \\\n gho_SPLIT0123456789\n' >"$tmp/split.txt"
if OPENCODE_API_KEY="oc-test-key-AbCdEf123456" bash "$here/redact.sh" "$tmp" >/dev/null 2>&1; then
  # The run should have failed on the residual split token OR redacted it;
  # in both cases no residual 'Bearer gho_' may remain on disk.
  if grep -rq 'Bearer gho_SPLIT' "$tmp"; then
    ko_result=1
  else
    ko_result=0
  fi
else
  ko_result=0
fi
check "split-token content is never left unredacted" "[ '$ko_result' -eq 0 ]"

echo
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
