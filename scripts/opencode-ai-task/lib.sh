#!/usr/bin/env bash
#
# Shared helpers for the OpenCode AI task automation scripts.
# Keep this file POSIX-friendly (must run under bash 3.2 and modern bash).

# sha256_of FILE -> prints the hex digest
sha256_of() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  else
    shasum -a 256 "$1" | cut -d' ' -f1
  fi
}

# write_checksums DIR -> writes SHA256SUMS over every regular file in DIR
# except SHA256SUMS itself.
write_checksums() {
  local dir="$1"
  (
    cd "$dir" || exit 1
    find . -type f ! -name SHA256SUMS -print | sed 's|^\./||' | LC_ALL=C sort |
    while IFS= read -r f; do
      if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$f"
      else
        shasum -a 256 "$f"
      fi
    done >SHA256SUMS
  )
}

# verify_checksums DIR -> exits 0 when every recorded checksum matches
verify_checksums() {
  local dir="$1"
  (
    cd "$dir" || exit 1
    [ -f SHA256SUMS ] || exit 1
    if command -v sha256sum >/dev/null 2>&1; then
      sha256sum -c SHA256SUMS
    else
      shasum -a 256 -c SHA256SUMS
    fi >/dev/null 2>&1
  )
}

# require_numeric VALUE -> exits 0 only when VALUE is a non-negative integer
require_numeric() {
  case "${1:-}" in
    '' | *[!0-9]*) return 1 ;;
    *) return 0 ;;
  esac
}

# json_string_value FILE KEY -> prints the value of a simple string field
# ("key": "value") from a single-line JSON object of our own making.
json_string_value() {
  sed -n 's/.*"'"$2"'"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' "$1" | head -n 1
}

# json_number_value FILE KEY -> prints the value of a simple numeric field
json_number_value() {
  sed -n 's/.*"'"$2"'"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' "$1" | head -n 1
}
