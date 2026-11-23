#!/usr/bin/env bash
#
# Redact secrets from diagnostic files before they are uploaded.
#
# Fail-closed: any conversion error, or any residual secret detected during
# verification, aborts with exit 1 so the caller can refuse to upload.
#
# Usage: redact.sh <dir> [dir...]
#
# Redacts, without echoing any secret:
#   - the value of OPENCODE_API_KEY (literal match, safe for keys containing
#     regex metacharacters),
#   - Authorization / WWW-Authenticate bearer-style headers,
#   - GitHub token literals (gho_, ghp_, ghs_, ghu_, github_pat_),
#   - x-access-token:<secret>@ embedded in URLs.

set -euo pipefail

[ "$#" -ge 1 ] || { echo "usage: redact.sh <dir> [dir...]" >&2; exit 1; }

KEY="${OPENCODE_API_KEY:-}"

# The key replacement runs through awk index()/substr(), which match literally
# and are immune to regex metacharacters in the key. The generic patterns use
# POSIX awk regular expressions.
redact_file() {
  local file="$1" tmp
  tmp="${file}.redact-tmp"
  awk -v key="${KEY}" -v marker="[REDACTED-OPENCODE-KEY]" '
    {
      s = $0
      if (key != "") {
        while ((i = index(s, key)) > 0) {
          s = substr(s, 1, i - 1) marker substr(s, i + length(key))
        }
      }
      gsub(/[Aa]uthorization( |:)*[Bb]earer [A-Za-z0-9._~+\/=-]+/, "[REDACTED-BEARER]", s)
      gsub(/gh[a-z]_[A-Za-z0-9_]+/, "[REDACTED-GITHUB-TOKEN]", s)
      gsub(/github_pat_[A-Za-z0-9_]+/, "[REDACTED-GITHUB-TOKEN]", s)
      gsub(/x-access-token:[^@\/]+@/, "x-access-token:[REDACTED]@", s)
      print s
    }' "${file}" >"${tmp}" || return 1
  mv -f "${tmp}" "${file}" || return 1
  return 0
}

# Residual-secret detection used for verification (never prints the secret).
verify_clean() {
  local file="$1"
  if [ -n "${KEY}" ] && grep -qF -- "${KEY}" "${file}"; then
    return 1
  fi
  if grep -Eq -- 'gh[a-z]_[A-Za-z0-9_]{8,}|github_pat_[A-Za-z0-9_]{8,}' "${file}"; then
    return 1
  fi
  if grep -Eq -- '[Aa]uthorization[: ]*[Bb]earer [A-Za-z0-9]' "${file}"; then
    return 1
  fi
  if grep -Eq -- 'x-access-token:[^[]+@' "${file}"; then
    return 1
  fi
  return 0
}

status=0
for dir in "$@"; do
  [ -d "${dir}" ] || continue
  while IFS= read -r -d '' file; do
    if ! redact_file "${file}"; then
      echo "::error::Redaction failed for ${file}; refusing to continue." >&2
      rm -f "${file}.redact-tmp" 2>/dev/null || true
      status=1
      continue
    fi
    if ! verify_clean "${file}"; then
      echo "::error::Residual secret detected after redaction in ${file}; refusing to continue." >&2
      status=1
    fi
  done < <(find "${dir}" -type f -print0)
done

if [ "${status}" -ne 0 ]; then
  echo "::error::Redaction did not complete safely. Do not upload these diagnostics." >&2
fi
exit "${status}"
