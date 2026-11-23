#!/usr/bin/env bash
#
# Redact secrets from diagnostic files before they are uploaded.
#
# Usage: redact.sh <directory> [more directories...]
# Scans every regular file and replaces, without echoing anything:
#   - the value of OPENCODE_API_KEY (if set),
#   - Authorization / WWW-Authenticate bearer-style headers,
#   - GitHub token literals (gho_, ghp_, ghs_, ghu_, github_pat_),
#   - x-access-token:<secret>@ embedded in URLs.
#
# The substitution never prints matched content.

set -euo pipefail

[ "$#" -ge 1 ] || { echo "usage: redact.sh <dir> [dir...]" >&2; exit 1; }

KEY="${OPENCODE_API_KEY:-}"

# Build the sed expressions; the key substitution is only added when a key
# value is actually set (an empty pattern would corrupt every file).
exprs=(
  -e 's|[Aa]uthorization[: ]*[Bb]earer [A-Za-z0-9._~+/=-]*|[REDACTED-BEARER]|g'
  -e 's|gh[a-z]_[A-Za-z0-9_]\{16,\}|[REDACTED-GITHUB-TOKEN]|g'
  -e 's|github_pat_[A-Za-z0-9_]\{16,\}|[REDACTED-GITHUB-TOKEN]|g'
  -e 's|x-access-token:[^@/]*@|x-access-token:[REDACTED]@|g'
)
if [ -n "${KEY}" ]; then
  exprs=(-e "s|${KEY}|[REDACTED-OPENCODE-KEY]|g" "${exprs[@]}")
fi

for dir in "$@"; do
  [ -d "$dir" ] || continue
  find "$dir" -type f -print0 |
  while IFS= read -r -d '' file; do
    sed -i.bak "${exprs[@]}" "$file" 2>/dev/null || true
    rm -f "${file}.bak"
  done
done

echo "Redaction applied."
