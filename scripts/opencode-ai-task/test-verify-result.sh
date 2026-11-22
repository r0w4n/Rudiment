#!/usr/bin/env bash
#
# Behavioural tests for scripts/opencode-ai-task/verify-result.sh.
#
# Builds throwaway local repositories (no network) and executes the guard
# script for real, covering the defect where standalone Markdown code-fence
# lines were treated as shell commands.
# Usage: bash scripts/opencode-ai-task/test-verify-result.sh

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
verify="$here/verify-result.sh"

pass=0
fail=0
tmp="$(mktemp -d /tmp/verify-result-tests.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT

run_guard() { # $1 = workdir; $2 = branch; $3 = default branch
  (cd "$1" && BRANCH="$2" DEFAULT_BRANCH="$3" bash "$verify") >"$tmp/stdout" 2>"$tmp/stderr" && rc=0 || rc=$?
}

check() { # $1=scenario $2=expected_rc(0|1) $3=actual_rc $4=optional pattern for stderr
  local scenario="$1" expected="$2" rc="$3" pattern="${4:-}"
  if [ "$rc" -eq "$expected" ]; then
    if [ -n "$pattern" ] && [ "$expected" -eq 1 ] && ! grep -q "$pattern" "$tmp/stderr"; then
      fail=$((fail + 1))
      echo "FAIL: $scenario (rc ok but stderr missing '$pattern')"
      return
    fi
    pass=$((pass + 1))
    echo "PASS: $scenario"
  else
    fail=$((fail + 1))
    echo "FAIL: $scenario (expected rc=$expected, got rc=$rc)"
    sed -n '1,5p' "$tmp/stderr" 2>/dev/null || true
  fi
}

setup_repo() { # $1 = workdir; creates origin, main, and task branch ai/issue-5 (at main's tip)
  local dir="$1"
  git init -q --bare "$dir/origin.git"
  git clone -q "$dir/origin.git" "$dir/work" 2>/dev/null
  (
    cd "$dir/work"
    git config user.email test@example.com
    git config user.name test
    echo base >README.md
    git add README.md
    git commit -qm "init"
    git branch -M main
    git push -q origin main
    git checkout -q -b ai/issue-5
    git checkout -q main
  )
}

commit_change() { # $1 = workdir; $2 = path; $3 = content; commits on the task branch
  (
    cd "$1/work"
    git checkout -q ai/issue-5
    mkdir -p "$(dirname "$2")"
    echo "$3" >"$2"
    git add "$2"
    git commit -qm "change"
  )
}

# 1. Allowed change on the task branch passes.
setup_repo "$tmp/t1"
commit_change "$tmp/t1" "src/lib.rs" "fn main() {}"
GITHUB_STEP_SUMMARY="$tmp/summary1.md" run_guard "$tmp/t1/work" ai/issue-5 main
check "clean implementation on task branch passes" 0 "$rc"
if [ -f "$tmp/summary1.md" ] && grep -q '^```$' "$tmp/summary1.md" && grep -q 'src/lib.rs' "$tmp/summary1.md"; then
  pass=$((pass + 1))
  echo "PASS: step summary renders changed files inside code fences"
else
  fail=$((fail + 1))
  echo "FAIL: step summary renders changed files inside code fences"
fi

# 2. Regression: a change under .github/ must be blocked.
setup_repo "$tmp/t2"
commit_change "$tmp/t2" ".github/workflows/evil.yml" "on: push"
run_guard "$tmp/t2/work" ai/issue-5 main
check "change under .github/ is blocked" 1 "$rc" "protected CI or security paths"

# 3. Regression: a change under .githooks/ must be blocked.
setup_repo "$tmp/t3"
commit_change "$tmp/t3" ".githooks/pre-commit" "# evil"
run_guard "$tmp/t3/work" ai/issue-5 main
check "change under .githooks/ is blocked" 1 "$rc" "protected CI or security paths"

# 4. Uncommitted changes must be blocked.
setup_repo "$tmp/t4"
commit_change "$tmp/t4" "src/lib.rs" "fn main() {}"
echo uncommitted >"$tmp/t4/work/stray.txt"
run_guard "$tmp/t4/work" ai/issue-5 main
check "uncommitted changes are blocked" 1 "$rc" "not clean"

# 5. A branch with no commits beyond the default branch must be blocked.
setup_repo "$tmp/t5"
commit_change "$tmp/t5" "src/lib.rs" "fn main() {}"
(
  cd "$tmp/t5/work"
  git reset -q --hard origin/main # put the task branch back at main's tip
)
run_guard "$tmp/t5/work" ai/issue-5 main
check "branch without new commits is blocked" 1 "$rc" "No commits"

# 6. Wrong current branch must be blocked.
setup_repo "$tmp/t6"
commit_change "$tmp/t6" "src/lib.rs" "fn main() {}"
(
  cd "$tmp/t6/work"
  git checkout -q main
)
run_guard "$tmp/t6/work" ai/issue-5 main
check "wrong current branch is blocked" 1 "$rc" "did not create or switch"

# 7. The guard must refuse to operate on the default branch itself.
setup_repo "$tmp/t7"
commit_change "$tmp/t7" "src/lib.rs" "fn main() {}"
(
  cd "$tmp/t7/work"
  git checkout -q main
)
BRANCH=main DEFAULT_BRANCH=main bash "$verify" >"$tmp/stdout" 2>"$tmp/stderr" && rc=0 || rc=$?
check "task branch equal to the default branch is blocked" 1 "$rc" "default branch"

# 8. Missing required environment fails loudly (configuration error).
BRANCH= DEFAULT_BRANCH=main bash "$verify" >"$tmp/stdout" 2>"$tmp/stderr" && rc=0 || rc=$?
check "missing BRANCH is rejected" 1 "$rc"

echo
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
