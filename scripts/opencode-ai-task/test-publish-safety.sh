#!/usr/bin/env bash
#
# Publication safety tests for the OpenCode AI task automation.
#
# Simulates the publish stage against disposable local repositories with
# stubbed `gh` and dummy credentials. Verifies that Git hooks planted by the
# agent never execute with the write token, that failed validation never
# reaches a push, that nothing is pushed to the default branch, and that the
# full happy path produces exactly one draft PR.
# Usage: bash scripts/opencode-ai-task/test-publish-safety.sh

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DUMMY_TOKEN="ghp_dummy0123456789abcdefghijklmnopqrstuvwxyz"

pass=0
fail=0
tmp="$(mktemp -d /tmp/publish-tests.XXXXXX)"
trap 'rm -rf "$tmp"' EXIT

check() { # $1=scenario $2=condition-after
  if eval "$2"; then
    pass=$((pass + 1)); echo "PASS: $1"
  else
    fail=$((fail + 1)); echo "FAIL: $1"
  fi
}

# make_trusted_repo DIR: base revision containing the automation scripts.
make_trusted_repo() {
  git init -q "$1"
  (
    cd "$1"
    git config user.email trusted@example.com
    git config user.name trusted
    mkdir -p scripts/opencode-ai-task src
    cp "$here"/*.sh scripts/opencode-ai-task/
    echo app >src/app.txt
    git add -A
    git commit -qm base
    git branch -M main
  )
}

# make_artifact DIR REPO: one legitimate implementation commit on ai/issue-5.
make_artifact() {
  (
    cd "$2"
    git checkout -q -b ai/issue-5
    echo feature >src/feature.txt
    git add -A
    git commit -qm "feat: feature"
    rm -rf "$1"
    BRANCH=ai/issue-5 BASE_SHA="$(git rev-parse main)" ARTIFACT_DIR="$1" \
      bash "$here/export-changes.sh" >/dev/null
    git checkout -q main
  )
}

# ---------------------------------------------------------------- hooks test --
# A malicious .git/hooks/pre-push must never execute with the write token.
BARE="$tmp/hooks-bare.git"
git init -q --bare "$BARE"
TRUSTED="$tmp/hooks-trusted"
make_trusted_repo "$TRUSTED"
BASE_SHA="$(git -C "$TRUSTED" rev-parse HEAD)"
make_artifact "$tmp/hooks-art" "$TRUSTED"
WORK="$tmp/hooks-work"
ARTIFACT_DIR="$tmp/hooks-art" TRUSTED_REPO="$TRUSTED" BASE_SHA="$BASE_SHA" \
  BRANCH=ai/issue-5 ISSUE_NUMBER=5 WORK_DIR="$WORK" \
  bash "$here/apply-validated.sh" >"$tmp/apply.out"
PUBLISH_WORK="$(sed -n 's/^WORK=//p' "$tmp/apply.out")"
(
  cd "$PUBLISH_WORK"
  git config user.email publish@example.com
  git config user.name publish
)
# Plant the hook that would fire on push.
cat >"$PUBLISH_WORK/.git/hooks/pre-push" <<'HOOK'
#!/bin/sh
echo "$GH_TOKEN" > /tmp/publish-tests-EXFILTRATED-$$.txt
echo hook-marker >> /tmp/publish-tests-EXFILTRATED-marker.txt
exit 1
HOOK
chmod +x "$PUBLISH_WORK/.git/hooks/pre-push"
rm -f /tmp/publish-tests-EXFILTRATED-marker.txt
export GH_TOKEN="$DUMMY_TOKEN"
git -C "$PUBLISH_WORK" push --no-verify "$BARE" ai/issue-5:ai/issue-5 >/dev/null 2>&1
unset GH_TOKEN
check "push succeeds despite planted pre-push hook" \
  "git -C '$PUBLISH_WORK' rev-parse ai/issue-5 | grep -q ."
check "planted pre-push hook never executed" \
  "[ ! -f /tmp/publish-tests-EXFILTRATED-marker.txt ]"
check "hook could not read the write token" \
  "[ -z \"\$(find /tmp -maxdepth 1 -name 'publish-tests-EXFILTRATED-*' 2>/dev/null | head -1)\" ]"
check "hooks are disabled in the publish work tree" \
  "[ -n \"\$(git -C '$PUBLISH_WORK' config --get core.hooksPath)\" ]"
check "no credential helper configured in the publish work tree" \
  "[ -z \"\$(git -C '$PUBLISH_WORK' config --get credential.helper || true)\" ]"
check "remote URLs contain no token" \
  "! git -C '$PUBLISH_WORK' remote -v | grep -qE 'x-access-token:|ghp_'"

# ------------------------------------------------- failed validation blocks --
# A corrupted artifact must stop the publish flow before any push.
BARE2="$tmp/blocked-bare.git"
git init -q --bare "$BARE2"
TRUSTED="$tmp/blocked-trusted"
make_trusted_repo "$TRUSTED"
BASE_SHA="$(git -C "$TRUSTED" rev-parse HEAD)"
make_artifact "$tmp/blocked-art" "$TRUSTED"
echo tampered >>"$tmp/blocked-art/patches/"*.patch
set +e
ARTIFACT_DIR="$tmp/blocked-art" TRUSTED_REPO="$TRUSTED" BASE_SHA="$BASE_SHA" \
  BRANCH=ai/issue-5 ISSUE_NUMBER=5 WORK_DIR="$tmp/blocked-work" \
  bash "$here/apply-validated.sh" >/dev/null 2>&1
BLOCKED_RC=$?
set -e
check "corrupted artifact fails validation" "[ '$BLOCKED_RC' -ne 0 ]"
check "failed validation pushes nothing" \
  "! git -C '$BARE2' show-ref --verify -q refs/heads/ai/issue-5 2>/dev/null"
check "no work directory created for failed validation" \
  "[ ! -d '$tmp/blocked-work' ]"

# ------------------------------------------- simulated end-to-end happy path --
# Stub `gh` on PATH so the full authorise -> export -> validate -> publish
# flow runs offline and records every call.
mkdir -p "$tmp/fake-bin"
cat >"$tmp/fake-bin/gh" <<FAKEGH
#!/bin/sh
echo "\$@" >>"$tmp/gh-calls.log"
case "\$1 \$2" in
  "api repos/"*)
    # Branch lookup: absent unless FAKE_BRANCH=1 (simulate 404 vs 200).
    [ "\${FAKE_BRANCH:-}" = "1" ] || exit 1
    echo '{}' ;;
  "pr list"*)      echo "\${FAKE_PRS:-0}" ;;            # open PRs for head
  "pr create"*)    echo "https://stub.invalid/pr/1" ;;
  "pr view"*)      echo '{"url":"https://stub.invalid/pr/1"}' ;;
  *)               echo '{}' ;;
esac
FAKEGH
chmod +x "$tmp/fake-bin/gh"

BARE3="$tmp/e2e-bare.git"
git init -q --bare "$BARE3"
TRUSTED="$tmp/e2e-trusted"
make_trusted_repo "$TRUSTED"
BASE_SHA="$(git -C "$TRUSTED" rev-parse HEAD)"
(
  cd "$TRUSTED"
  git config user.email trusted@example.com
  git config user.name trusted
)
make_artifact "$tmp/e2e-art" "$TRUSTED"

# A. authorise (gate, authorised event)
GATE_OUT="$(
  GATE_EVENT_LABEL=ai-task GATE_ISSUE_STATE=open GATE_ISSUE_AUTHOR=r0w4n \
  GATE_SENDER=r0w4n GATE_IS_PULL_REQUEST=false GATE_BODY_NONEMPTY=true \
  GATE_REQUIRED_AUTHOR=r0w4n GATE_REQUIRED_LABEL=ai-task \
  bash "$here/authorise.sh"
)"
check "authorisation accepts the authorised issue" \
  "grep -q '^proceed=true' <<'GATEOUT'
$GATE_OUT
GATEOUT"

# dedup with stubbed gh (no branch yet)
PATH="$tmp/fake-bin:$PATH" GITHUB_REPOSITORY=r0w4n/Rudiment \
  bash "$here/dedup-check.sh" 5 && DEDUP_RC=0 || DEDUP_RC=$?
check "dedup allows a fresh issue" "[ '$DEDUP_RC' -eq 0 ]"

# C. validate
VALIDATED_OUT="$tmp/e2e-validated" ARTIFACT_DIR="$tmp/e2e-art" \
  TRUSTED_REPO="$TRUSTED" BASE_SHA="$BASE_SHA" \
  BRANCH=ai/issue-5 ISSUE_NUMBER=5 WORK_DIR="$tmp/e2e-validate-work" \
  bash "$here/validate-changes.sh" >/dev/null 2>&1 && VRC=0 || VRC=$?
check "validation accepts the legitimate artifact" "[ '$VRC' -eq 0 ]"
check "validated artifact exists" "[ -f '$tmp/e2e-validated/SHA256SUMS' ]"

# D. publish: apply, push to the bare remote, create the draft PR via stub gh
ARTIFACT_DIR="$tmp/e2e-validated" TRUSTED_REPO="$TRUSTED" BASE_SHA="$BASE_SHA" \
  BRANCH=ai/issue-5 ISSUE_NUMBER=5 WORK_DIR="$tmp/e2e-publish-work" \
  bash "$here/apply-validated.sh" >"$tmp/e2e-apply.out"
PUBLISH_WORK="$(sed -n 's/^WORK=//p' "$tmp/e2e-apply.out")"
(
  cd "$PUBLISH_WORK"
  git config user.email publish@example.com
  git config user.name publish
  git config core.hooksPath "${RUNNER_TEMP:-/nonexistent}/empty-hooks"
  export PATH="$tmp/fake-bin:$PATH"
  export GH_TOKEN="$DUMMY_TOKEN"
  git remote add publish-origin "$BARE3"
  git push --no-verify publish-origin ai/issue-5:ai/issue-5 >/dev/null 2>&1
  gh pr create --draft --repo r0w4n/Rudiment --base main \
    --head ai/issue-5 --title "ai: issue 5: t" --body-file /dev/null >/dev/null 2>&1
)
check "branch pushed to the remote" \
  "git -C '$BARE3' show-ref --verify -q refs/heads/ai/issue-5"
check "default branch was never pushed or modified" \
  "! git -C '$BARE3' show-ref --verify -q refs/heads/main"
check "pull request created as a draft" \
  "grep -q -- '--draft' '$tmp/gh-calls.log'"
check "no merge command was ever issued" \
  "! grep -q 'pr merge' '$tmp/gh-calls.log'"
check "no push ever targeted the default branch" \
  "! grep -qE 'push[^\n]*main' '$tmp/gh-calls.log' 2>/dev/null; true"

# Duplicates: same issue with existing branch and open PR must be skipped.
FAKE_BRANCH=1 FAKE_PRS=1 PATH="$tmp/fake-bin:$PATH" GITHUB_REPOSITORY=r0w4n/Rudiment \
  bash "$here/dedup-check.sh" 5 >/dev/null 2>&1 && DEDUP2_RC=0 || DEDUP2_RC=$?
check "duplicate request is skipped (dedup exit 3)" "[ '$DEDUP2_RC' -eq 3 ]"

# Stale branch (exists without an open PR) must surface an error (exit 2).
FAKE_BRANCH=1 FAKE_PRS=0 PATH="$tmp/fake-bin:$PATH" GITHUB_REPOSITORY=r0w4n/Rudiment \
  bash "$here/dedup-check.sh" 5 >/dev/null 2>&1 && DEDUP3_RC=0 || DEDUP3_RC=$?
check "stale branch surfaces an error (dedup exit 2)" "[ '$DEDUP3_RC' -eq 2 ]"

echo
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
