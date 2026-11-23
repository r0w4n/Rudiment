#!/usr/bin/env bash
#
# Adversarial tests for scripts/opencode-ai-task/validate-changes.sh.
#
# Builds disposable local repositories and deliberately malicious artifacts
# (no network, no credentials, no model calls) and verifies that validation
# fails closed in every attack scenario.
# Usage: bash scripts/opencode-ai-task/test-validate-changes.sh

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
validate="$here/validate-changes.sh"

pass=0
fail=0
tmp="$(mktemp -d /tmp/validate-tests.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT

# make_trusted_repo DIR
#   Creates a repository resembling the merged automation: a base revision
#   that contains the trusted automation scripts plus ordinary app files.
make_trusted_repo() {
  local dir="$1"
  git init -q "$dir"
  (
    cd "$dir"
    git config user.email trusted@example.com
    git config user.name trusted
    mkdir -p scripts/opencode-ai-task src
    cp "$here"/*.sh "$here/../.."/scripts/opencode-ai-task/*.sh scripts/opencode-ai-task/ 2>/dev/null || true
    echo app >src/app.txt
    echo "trusted automation" >scripts/opencode-ai-task/.trusted-marker
    git add -A
    git commit -qm "base"
  )
  git -C "$dir" rev-parse HEAD
}

# make_artifact DIR REPO_PATH
#   Exports a real artifact from the given repository's task branch using
#   the global BASE_SHA.
make_artifact() {
  (
    cd "$2"
    rm -rf "$1"
    BRANCH=ai/issue-5 BASE_SHA="${BASE_SHA:?}" ARTIFACT_DIR="$1" \
      bash "$here/export-changes.sh" >/dev/null 2>&1
    # Restore the trusted repo to the base revision, as a real checkout of
    # the base ref would be (validate clones it and asserts HEAD == base).
    git checkout -q "${BASE_SHA:?}"
  )
}

run_validate() { # $1 = artifact dir; $2 = trusted repo; $3 = branch (default ai/issue-5)
  ARTIFACT_DIR="$1" TRUSTED_REPO="$2" BASE_SHA="$BASE_SHA" \
    BRANCH="${3:-ai/issue-5}" ISSUE_NUMBER=5 \
    WORK_DIR="$tmp/work" VALIDATED_OUT="${VALIDATED_OUT:-}" \
    bash "$validate" >"$tmp/stdout" 2>"$tmp/stderr" && rc=0 || rc=$?
}

# make_meta DIR
#   Writes a fresh meta.json for the current BASE_SHA (checksums must be
#   regenerated afterwards).
make_meta() {
  cat >"$1/meta.json" <<EOF
{"issue_number": "5", "branch": "ai/issue-5", "base_sha": "${BASE_SHA}", "head_sha": "deadbeef00000000000000000000000000000000", "commit_count": 1, "model": "test", "validated": "false"}
EOF
}

check_reject() { # $1 = scenario; $2 = expected stderr pattern
  if [ "$rc" -eq 1 ] && grep -q "$2" "$tmp/stderr"; then
    pass=$((pass + 1)); echo "PASS: $1"
  else
    fail=$((fail + 1)); echo "FAIL: $1 (rc=$rc)"; sed -n '1,3p' "$tmp/stderr" 2>/dev/null || true
  fi
}

check_accept() { # $1 = scenario
  if [ "$rc" -eq 0 ]; then
    pass=$((pass + 1)); echo "PASS: $1"
  else
    fail=$((fail + 1)); echo "FAIL: $1 (rc=$rc)"; sed -n '1,3p' "$tmp/stderr" 2>/dev/null || true
  fi
}

# agent_commit TRUSTED_REPO: adds one commit on ai/issue-5 writing $2 to $1
agent_commit() {
  (
    cd "$1"
    git checkout -q -b ai/issue-5 2>/dev/null || git checkout -q ai/issue-5
    mkdir -p "$(dirname "$2")"
    printf '%s\n' "$3" >"$2"
    git add -A
    git commit -qm "malicious: $2"
  )
}

# --- baseline: a legitimate implementation is accepted ------------------------
TRUSTED="$tmp/trusted0"
BASE_SHA="$(make_trusted_repo "$TRUSTED")"
(
  cd "$TRUSTED"
  git checkout -q -b ai/issue-5
  echo feature >src/feature.txt
  git add src/feature.txt
  git commit -qm "feat: feature"
)
make_artifact "$tmp/art0" "$TRUSTED"
VALIDATED_OUT="$tmp/validated0" run_validate "$tmp/art0" "$TRUSTED"
check_accept "legitimate implementation is accepted"
if [ -f "$tmp/validated0/SHA256SUMS" ] && find "$tmp/validated0" -type f -path '*.git*' | grep -q .; then
  fail=$((fail + 1)); echo "FAIL: validated artifact has checksums and no .git data"
else
  pass=$((pass + 1)); echo "PASS: validated artifact has checksums and no .git data"
fi

# --- attack 1: AI-modified verification script must not bypass checks --------
TRUSTED="$tmp/trusted1"
BASE_SHA="$(make_trusted_repo "$TRUSTED")"
agent_commit "$TRUSTED" "scripts/opencode-ai-task/validate-changes.sh" 'exit 0  # tampered'
make_artifact "$tmp/art1" "$TRUSTED"
run_validate "$tmp/art1" "$TRUSTED"
check_reject "tampered verification script is rejected" "protected paths"

# --- attack 2: changes to .github/ must be rejected ---------------------------
TRUSTED="$tmp/trusted2"
BASE_SHA="$(make_trusted_repo "$TRUSTED")"
agent_commit "$TRUSTED" ".github/workflows/evil.yml" "on: push"
make_artifact "$tmp/art2" "$TRUSTED"
run_validate "$tmp/art2" "$TRUSTED"
check_reject "change under .github/ is rejected" "protected paths"

# --- attack 3: changes to .githooks/ must be rejected -------------------------
TRUSTED="$tmp/trusted3"
BASE_SHA="$(make_trusted_repo "$TRUSTED")"
agent_commit "$TRUSTED" ".githooks/pre-push" "# evil"
make_artifact "$tmp/art3" "$TRUSTED"
run_validate "$tmp/art3" "$TRUSTED"
check_reject "change under .githooks/ is rejected" "protected paths"

# --- attack 4: patch planting a symlink must be rejected ----------------------
TRUSTED="$tmp/trusted4"
BASE_SHA="$(make_trusted_repo "$TRUSTED")"
agent_commit "$TRUSTED" "src/app.txt" "app-modified"
(
  cd "$TRUSTED"
  ln -s /etc/passwd src/link
  git add src/link
  git commit -qm "evil: symlink"
)
make_artifact "$tmp/art4" "$TRUSTED"
run_validate "$tmp/art4" "$TRUSTED"
check_reject "symlink patch is rejected" "symlinks or submodules"

# --- attack 5: patch adding a submodule must be rejected ----------------------
TRUSTED="$tmp/trusted5"
BASE_SHA="$(make_trusted_repo "$TRUSTED")"
agent_commit "$TRUSTED" "src/app.txt" "app-modified"
# Hand-craft a patch that adds a gitlink, since format-patch of a real
# submodule requires one; the malicious artifact is the point.
mkdir -p "$tmp/art5/patches"
cat >"$tmp/art5/patches/0001-evil.patch" <<PATCH
From 0000000000000000000000000000000000000000 Mon Sep 17 00:00:00 2001
Subject: [PATCH] evil submodule
---
 .gitmodules        | 3 +++
 evil-sub           | 1 +
 2 files changed, 3 insertions(+)
 create mode 160000 evil-sub
 create mode 100644 .gitmodules
diff --git a/.gitmodules b/.gitmodules
new file mode 100644
index 0000000..e69de29
--- /dev/null
+++ b/.gitmodules
@@ -0,0 +1,3 @@
+[submodule "evil"]
+	path = evil-sub
+	url = https://evil.example/x
diff --git a/evil-sub b/evil-sub
new file mode 160000
index 0000000000000000000000000000000000000000..
--- /dev/null
+++ b/evil-sub
@@ -0,0 +1 @@
+Subproject commit 0000000000000000000000000000000000000000
--
2.39.0
PATCH
make_meta "$tmp/art5"
printf 'x\n' >"$tmp/art5/pr-summary.md"; printf 't\n' >"$tmp/art5/issue-title.txt"
(cd "$tmp/art5" && shasum -a 256 patches/0001-evil.patch meta.json pr-summary.md issue-title.txt >SHA256SUMS)
run_validate "$tmp/art5" "$TRUSTED"
check_reject "submodule patch is rejected" "symlinks or submodules"

# --- attack 6: patch touching .git internals must be rejected -----------------
TRUSTED="$tmp/trusted6"
BASE_SHA="$(make_trusted_repo "$TRUSTED")"
agent_commit "$TRUSTED" "src/app.txt" "app-modified"
mkdir -p "$tmp/art6/patches"
cat >"$tmp/art6/patches/0001-evil.patch" <<PATCH
From 0000000000000000000000000000000000000000 Mon Sep 17 00:00:00 2001
Subject: [PATCH] evil git config
---
 .git/config | 2 ++
 1 file changed, 2 insertions(+)
diff --git a/.git/config b/.git/config
index 0000000..1111111 100644
--- a/.git/config
+++ b/.git/config
@@ -0,0 +1,2 @@
+[credential]
+	helper = store
--
2.39.0
PATCH
make_meta "$tmp/art6"
printf 'x\n' >"$tmp/art6/pr-summary.md"; printf 't\n' >"$tmp/art6/issue-title.txt"
(cd "$tmp/art6" && shasum -a 256 patches/0001-evil.patch meta.json pr-summary.md issue-title.txt >SHA256SUMS)
run_validate "$tmp/art6" "$TRUSTED"
check_reject "patch touching .git internals is rejected" "Git-internal paths"

# --- attack 7: path traversal must be rejected --------------------------------
TRUSTED="$tmp/trusted7"
BASE_SHA="$(make_trusted_repo "$TRUSTED")"
agent_commit "$TRUSTED" "src/app.txt" "app-modified"
mkdir -p "$tmp/art7/patches"
cat >"$tmp/art7/patches/0001-evil.patch" <<PATCH
From 0000000000000000000000000000000000000000 Mon Sep 17 00:00:00 2001
Subject: [PATCH] evil traversal
---
 ../outside.txt | 1 +
 1 file changed, 1 insertion(+)
diff --git a/../outside.txt b/../outside.txt
new file mode 100644
index 0000000..e69de29
--- /dev/null
+++ b/../outside.txt
@@ -0,0 +1 @@
+owned
--
2.39.0
PATCH
make_meta "$tmp/art7"
printf 'x\n' >"$tmp/art7/pr-summary.md"; printf 't\n' >"$tmp/art7/issue-title.txt"
(cd "$tmp/art7" && shasum -a 256 patches/0001-evil.patch meta.json pr-summary.md issue-title.txt >SHA256SUMS)
run_validate "$tmp/art7" "$TRUSTED"
check_reject "path traversal is rejected" "traversal"

# --- attack 8: corrupted artifact (checksum mismatch) must be rejected --------
TRUSTED="$tmp/trusted8"
BASE_SHA="$(make_trusted_repo "$TRUSTED")"
(
  cd "$TRUSTED"
  git checkout -q -b ai/issue-5
  echo feature >src/feature.txt
  git add src/feature.txt
  git commit -qm "feat: feature"
)
make_artifact "$tmp/art8" "$TRUSTED"
echo tampered >>"$tmp/art8/patches/"*.patch
run_validate "$tmp/art8" "$TRUSTED"
check_reject "checksum mismatch is rejected" "checksum"

# --- attack 9: base SHA mismatch must be rejected (fail closed) ---------------
TRUSTED="$tmp/trusted9"
BASE_SHA="$(make_trusted_repo "$TRUSTED")"
(
  cd "$TRUSTED"
  git checkout -q -b ai/issue-5
  echo feature >src/feature.txt
  git add src/feature.txt
  git commit -qm "feat: feature"
)
make_artifact "$tmp/art9" "$TRUSTED"
BASE_SHA="0000000000000000000000000000000000000000" \
  run_validate "$tmp/art9" "$TRUSTED"
check_reject "base SHA mismatch is rejected" "base_sha mismatch"

# --- attack 10: malformed patch (unappliable) must be rejected ----------------
TRUSTED="$tmp/trusted10"
BASE_SHA="$(make_trusted_repo "$TRUSTED")"
agent_commit "$TRUSTED" "src/app.txt" "app-modified"
(cd "$TRUSTED" && git checkout -q "${BASE_SHA}") # restore base HEAD, as a real checkout would
mkdir -p "$tmp/art10/patches"
printf 'this is not a patch\n' >"$tmp/art10/patches/0001-broken.patch"
make_meta "$tmp/art10"
printf 'x\n' >"$tmp/art10/pr-summary.md"; printf 't\n' >"$tmp/art10/issue-title.txt"
(cd "$tmp/art10" && shasum -a 256 patches/0001-broken.patch meta.json pr-summary.md issue-title.txt >SHA256SUMS)
run_validate "$tmp/art10" "$TRUSTED"
check_reject "malformed patch is rejected" "could not be applied"

# --- attack 11: branch/issue mismatch must be rejected ------------------------
TRUSTED="$tmp/trusted11"
BASE_SHA="$(make_trusted_repo "$TRUSTED")"
(
  cd "$TRUSTED"
  git checkout -q -b ai/issue-5
  echo feature >src/feature.txt
  git add src/feature.txt
  git commit -qm "feat: feature"
)
make_artifact "$tmp/art11" "$TRUSTED"
run_validate "$tmp/art11" "$TRUSTED" ai/issue-9
check_reject "branch/issue mismatch is rejected" "does not match"

echo
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
