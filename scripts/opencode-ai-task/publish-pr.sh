#!/usr/bin/env bash
#
# Publication sequence for the OpenCode AI task: validate metadata, build the
# draft pull request body from the VALIDATED artifact, push the branch with
# hooks and credential helpers disabled, and create a draft pull request.
#
# Runs in the publish job from a TRUSTED checkout. GitHub credentials are
# supplied via GIT_ASKPASS only; the token never appears in a remote URL or
# Git configuration. If pull request creation fails after the push, the
# pushed branch is deleted again so no orphaned branch remains.
#
# Inputs (environment):
#   PUBLISH_WORK      working clone with the validated changes applied
#   ARTIFACT_DIR      downloaded VALIDATED artifact (meta.json, patches/,
#                     pr-summary.md, issue-title.txt)
#   BRANCH, ISSUE_NUMBER, BASE_SHA, DEFAULT_BRANCH
#   GH_TOKEN          write token (via askpass; not stored anywhere)
#   RUN_URL           optional workflow run link for the PR body
#   NOTIFY_GH         set to 0 in tests to skip calls that need gh
# Outputs:
#   Prints the created PR URL as "PR_URL=<url>" on success.
# Exits 1 (fail closed) before pushing whenever anything is invalid.

set -euo pipefail

. "$(dirname "$0")/lib.sh"

PUBLISH_WORK="${PUBLISH_WORK:?PUBLISH_WORK is required}"
ARTIFACT_DIR="${ARTIFACT_DIR:?ARTIFACT_DIR is required}"
BRANCH="${BRANCH:?BRANCH is required}"
ISSUE_NUMBER="${ISSUE_NUMBER:?ISSUE_NUMBER is required}"
BASE_SHA="${BASE_SHA:?BASE_SHA is required}"
DEFAULT_BRANCH="${DEFAULT_BRANCH:?DEFAULT_BRANCH is required}"
REPOSITORY="${REPOSITORY:?REPOSITORY is required}"
# Testable override: the complete remote URL. Defaults to the real GitHub.
REMOTE_URL="${REMOTE_URL:-https://github.com/${REPOSITORY}.git}"
RUN_URL="${RUN_URL:-}"
PR_BODY_FILE="${PR_BODY_FILE:-${RUNNER_TEMP:-/tmp}/opencode-pr-body.md}"

fail() { echo "::error::$1" >&2; exit 1; }

require_numeric "${ISSUE_NUMBER}" || fail "issue number is not numeric"

# --- Validate the artifact metadata BEFORE any push ---------------------------
META="${ARTIFACT_DIR}/meta.json"
[ -f "${META}" ] || fail "meta.json is missing from the validated artifact"
META_ISSUE="$(json_string_value "${META}" issue_number)"
META_BRANCH="$(json_string_value "${META}" branch)"
META_BASE="$(json_string_value "${META}" base_sha)"
META_MODEL="$(json_string_value "${META}" model)"
[ "${META_ISSUE}" = "${ISSUE_NUMBER}" ] ||
  fail "artifact issue (${META_ISSUE:-<missing>}) does not match the task issue (${ISSUE_NUMBER})"
[ "${META_BRANCH}" = "${BRANCH}" ] ||
  fail "artifact branch (${META_BRANCH:-<missing>}) does not match the task branch (${BRANCH})"
[ "${META_BASE}" = "${BASE_SHA}" ] ||
  fail "artifact base does not match the pinned base revision"
require_numeric "${META_ISSUE}" || fail "artifact issue number is invalid"

TITLE_FILE="${ARTIFACT_DIR}/issue-title.txt"
[ -f "${TITLE_FILE}" ] || fail "issue-title.txt is missing from the validated artifact"
ISSUE_TITLE="$(head -n 1 "${TITLE_FILE}")"
[ -n "${ISSUE_TITLE}" ] || fail "issue title in the validated artifact is empty"

SUMMARY_FILE="${ARTIFACT_DIR}/pr-summary.md"
[ -f "${SUMMARY_FILE}" ] || fail "pr-summary.md is missing from the validated artifact"
SUMMARY="$(cat "${SUMMARY_FILE}")"

# --- Build the pull request body from the validated artifact ------------------
{
  echo "## Summary"
  echo
  echo "${SUMMARY}"
  echo
  echo "## Automation notes"
  echo
  echo "- Created automatically by OpenCode from issue #${ISSUE_NUMBER} (model: \`${META_MODEL:-unknown}\`)."
  echo "- Draft pull request: nothing is merged, deployed or pushed to \`${DEFAULT_BRANCH}\` until you review and approve it."
  echo "- CI checks for this PR were created by \`GITHUB_TOKEN\` and are held for approval; approve the workflow runs from the pull request page to start the repository checks."
  if [ -n "${RUN_URL}" ]; then
    echo "- Workflow run: ${RUN_URL}"
  fi
  echo
  echo "Closes #${ISSUE_NUMBER}"
} >"${PR_BODY_FILE}"

# --- Git hygiene before anything authenticated --------------------------------
git -C "${PUBLISH_WORK}" config credential.helper ""
NO_HOOKS_DIR="${PUBLISH_WORK}-no-hooks"
mkdir -p "${NO_HOOKS_DIR}"
git -C "${PUBLISH_WORK}" config core.hooksPath "${NO_HOOKS_DIR}"

# --- Credentials via askpass only ---------------------------------------------
ASKPASS="${RUNNER_TEMP:-/tmp}/git-askpass.$$.sh"
cat >"${ASKPASS}" <<'ASKPASS'
#!/bin/sh
case "$1" in
  Username*) printf '%s\n' "x-access-token" ;;
  *)         printf '%s\n' "${GH_TOKEN}" ;;
esac
ASKPASS
chmod 700 "${ASKPASS}"
export GIT_ASKPASS="${ASKPASS}"
export GIT_TERMINAL_PROMPT=0

git -C "${PUBLISH_WORK}" remote add publish-origin "${REMOTE_URL}" 2>/dev/null ||
  git -C "${PUBLISH_WORK}" remote set-url publish-origin "${REMOTE_URL}"
cleanup() {
  git -C "${PUBLISH_WORK}" remote remove publish-origin 2>/dev/null || true
  rm -f "${ASKPASS}"
}
trap cleanup EXIT

# --- Push the task branch only ------------------------------------------------
git -C "${PUBLISH_WORK}" push --no-verify publish-origin "${BRANCH}:${BRANCH}" ||
  fail "pushing ${BRANCH} failed"

# --- Create the draft PR; clean up the branch if that fails -------------------
if ! gh pr create --draft \
    --repo "${REPOSITORY}" \
    --base "${DEFAULT_BRANCH}" \
    --head "${BRANCH}" \
    --title "ai: issue #${ISSUE_NUMBER}: ${ISSUE_TITLE}" \
    --body-file "${PR_BODY_FILE}"; then
  echo "::error::Draft pull request creation failed after the branch push."
  if git -C "${PUBLISH_WORK}" push --no-verify publish-origin --delete "${BRANCH}" >/dev/null 2>&1; then
    echo "::error::The pushed branch ${BRANCH} was deleted again so no orphaned branch remains."
    echo "::error::The validated implementation is retained as the 'opencode-validated' artifact of this run; fix the cause and rerun the task (dedup will then allow a fresh branch)."
  else
    echo "::error::The branch ${BRANCH} could not be deleted automatically. Delete it manually: git push origin --delete ${BRANCH}"
  fi
  exit 1
fi

cleanup

PR_URL="$(gh pr view "${BRANCH}" --repo "${REPOSITORY}" --json url --jq '.url')"
echo "PR_URL=${PR_URL}"
echo "::notice::Draft pull request created: ${PR_URL}"
echo "::notice::Its CI workflow runs are held for approval; approve them from the pull request page to start the repository checks."
