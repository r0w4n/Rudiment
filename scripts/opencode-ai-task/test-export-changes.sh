#!/usr/bin/env bash
#
# Behavioural tests for scripts/opencode-ai-task/export-changes.sh.
#
# Uses disposable local repositories only (no network, no credentials).
# Usage: bash scripts/opencode-ai-task/test-export-changes.sh

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export_script="$here/export-changes.sh"

pass=0
fail=0
tmp="$(mktemp -d /tmp/export-tests.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT

check() { # $1=scenario $2=expected_rc $3=actual_rc [$4=stderr pattern]
  local scenario="$1" expected="$2" rc="$3" pattern="${4:-}"
  if [ "$rc" -eq "$expected" ]; then
    if [ -n "$pattern" ] && [ "$expected" -ne 0 ] && ! grep -q "$pattern" "$tmp/stderr"; then
      fail=$((fail + 1))
      echo "FAIL: $scenario (rc ok but stderr missing '$pattern')"
      return
    fi
    pass=$((pass + 1))
    echo "PASS: $scenario"
  else
    fail=$((fail + 1))
    echo "FAIL: $scenario (expected rc=$expected, got rc=$rc)"
    sed -n '1,4p' "$tmp/stderr" 2>/dev/null || true
  fi
}

setup_repo() { # $1 = dir; base commit on main; empty task branch ai/issue-5
  git init -q --bare "$1/origin.git"
  git clone -q "$1/origin.git" "$1/work" 2>/dev/null
  (
    cd "$1/work"
    git config user.email test@example.com
    git config user.name test
    echo base >README.md
    mkdir -p scripts/opencode-ai-task
    echo trusted >scripts/opencode-ai-task/lib.sh
    git add -A
    git commit -qm init
    git branch -M main
    git push -q origin main
    BASE_SHA="$(git rev-parse HEAD)"
    git checkout -q -b ai/issue-5
  )
}

commit_on_task() { # $1 = dir; $2 = path; $3 = content
  (
    cd "$1/work"
    git checkout -q ai/issue-5
    mkdir -p "$(dirname "$2")"
    printf '%s\n' "$3" >"$2"
    git add -A
    git commit -qm "implement: $2"
  )
}

run_export() { # $1 = dir
  (
    cd "$1/work"
    rm -rf "$1/artifact"
    BRANCH=ai/issue-5 BASE_SHA="$(git rev-parse main)" \
      ARTIFACT_DIR="$1/artifact" \
      PR_SUMMARY_FILE="$1/summary.md" ISSUE_TITLE_FILE="$1/title.txt" \
      bash "$export_script"
  ) >"$tmp/stdout" 2>"$tmp/stderr" && rc=0 || rc=$?
}

# 1. A clean, linear implementation exports a correct artifact.
setup_repo "$tmp/t1"
printf 'the summary\n' >"$tmp/t1/summary.md"
printf 'The title\n' >"$tmp/t1/title.txt"
commit_on_task "$tmp/t1" "src/app.rs" "fn run() {}"
commit_on_task "$tmp/t1" "src/extra.rs" "fn extra() {}"
run_export "$tmp/t1"
check "clean implementation exports artifact" 0 "$rc"
if [ -f "$tmp/t1/artifact/patches/0001-implement-src-app.rs.patch" ] \
  && [ "$(find "$tmp/t1/artifact/patches" -name '*.patch' | wc -l | tr -d ' ')" = "2" ] \
  && [ -f "$tmp/t1/artifact/meta.json" ] \
  && [ -f "$tmp/t1/artifact/SHA256SUMS" ] \
  && [ -f "$tmp/t1/artifact/pr-summary.md" ] \
  && [ -f "$tmp/t1/artifact/issue-title.txt" ]; then
  pass=$((pass + 1)); echo "PASS: artifact contains patches, meta, summary, title and checksums"
else
  fail=$((fail + 1)); echo "FAIL: artifact contents"
fi
(
  cd "$tmp/t1/artifact" && shasum -a 256 -c SHA256SUMS >/dev/null 2>&1
) && { pass=$((pass + 1)); echo "PASS: SHA256SUMS verifies"; } \
  || { fail=$((fail + 1)); echo "FAIL: SHA256SUMS verifies"; }
if find "$tmp/t1/artifact" -type f -path '*.git*' | grep -q .; then
  fail=$((fail + 1)); echo "FAIL: no .git metadata in artifact"
else
  pass=$((pass + 1)); echo "PASS: no .git metadata in artifact"
fi

# 2. Dirty tree is refused.
setup_repo "$tmp/t2"
commit_on_task "$tmp/t2" "src/app.rs" "fn run() {}"
echo stray >"$tmp/t2/work/stray.txt"
run_export "$tmp/t2"
check "dirty tree refused" 1 "$rc" "not clean"

# 3. No commits is refused.
setup_repo "$tmp/t3"
run_export "$tmp/t3"
check "branch without commits refused" 1 "$rc" "no commits"

# 4. Merge commits are refused.
setup_repo "$tmp/t4"
commit_on_task "$tmp/t4" "src/app.rs" "fn run() {}"
(
  cd "$tmp/t4/work"
  git checkout -q main
  echo side >side.txt
  git add side.txt
  git commit -qm side
  git checkout -q ai/issue-5
  git merge -q --no-edit main
)
run_export "$tmp/t4"
check "merge commit refused" 1 "$rc" "linear history"

# 5. Wrong current branch is refused.
setup_repo "$tmp/t5"
commit_on_task "$tmp/t5" "src/app.rs" "fn run() {}"
(cd "$tmp/t5/work" && git checkout -q main)
run_export "$tmp/t5"
check "wrong current branch refused" 1 "$rc" "did not finish"

# 6. Task branch equal to the default branch is refused.
setup_repo "$tmp/t6"
commit_on_task "$tmp/t6" "src/app.rs" "fn run() {}"
(
  cd "$tmp/t6/work"
  rm -rf "$tmp/t6/artifact"
  BRANCH=main BASE_SHA="$(git rev-parse main^)" \
    ARTIFACT_DIR="$tmp/t6/artifact" \
    bash "$export_script"
) >"$tmp/stdout" 2>"$tmp/stderr" && rc=0 || rc=$?
check "task branch equal to default branch refused" 1 "$rc" "does not match"

# 7. A binary file change round-trips through format-patch --binary.
setup_repo "$tmp/t7"
printf 'BIN\x00\x01\x02\xff\xfe' >"$tmp/t7/blob.bin"
commit_on_task "$tmp/t7" "assets/blob.bin" "placeholder"
(
  cd "$tmp/t7/work"
  git checkout -q ai/issue-5
  printf 'BIN\x00\x01\x02\xff\xfe' >assets/blob.bin
  git add assets/blob.bin
  git commit -qm "add binary"
)
run_export "$tmp/t7"
check "binary change exports" 0 "$rc"
(
  cd "$tmp/t7/work"
  git checkout -q main
  git am -q "$tmp/t7/artifact/patches/"*.patch >/dev/null 2>&1
  [ "$(git show HEAD:assets/blob.bin | xxd -p)" = "$(printf 'BIN\x00\x01\x02\xff\xfe' | xxd -p)" ]
) && { pass=$((pass + 1)); echo "PASS: binary content round-trips"; } \
  || { fail=$((fail + 1)); echo "FAIL: binary content round-trips"; }

echo
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
