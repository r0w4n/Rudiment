#!/usr/bin/env bash
#
# Build and post the failure notification comment for the OpenCode AI task.
#
# The stage results are passed in through environment variables by the
# workflow (GitHub Actions expressions are evaluated there; this script only
# uses plain shell variables).
#
# Inputs (environment):
#   IMPLEMENT_RESULT  needs.implement.result  ("failure", "skipped", ...)
#   VALIDATE_RESULT   needs.validate.result
#   PUBLISH_RESULT    needs.publish.result
#   ISSUE_NUMBER      the originating issue
#   BRANCH            task branch (ai/issue-<n>)
#   DEFAULT_BRANCH    repository default branch
#   RUN_URL           workflow run URL
#   COMMENT_FILE      where to write the comment (default under RUNNER_TEMP)
#   NOTIFY_DRY_RUN    "1" builds the comment without calling gh (tests)
#   GH_TOKEN          required unless NOTIFY_DRY_RUN=1
#
# A failure of the gh call is reported but must never conceal the original
# failure: the workflow marks this step continue-on-error.

set -euo pipefail

ISSUE_NUMBER="${ISSUE_NUMBER:?ISSUE_NUMBER is required}"
BRANCH="${BRANCH:-}"
DEFAULT_BRANCH="${DEFAULT_BRANCH:-main}"
IMPLEMENT_RESULT="${IMPLEMENT_RESULT:-success}"
VALIDATE_RESULT="${VALIDATE_RESULT:-success}"
PUBLISH_RESULT="${PUBLISH_RESULT:-success}"
RUN_URL="${RUN_URL:-}"
COMMENT_FILE="${COMMENT_FILE:-${RUNNER_TEMP:-/tmp}/opencode-failure-comment.md}"
NOTIFY_DRY_RUN="${NOTIFY_DRY_RUN:-0}"

{
  echo "The OpenCode AI task for this issue failed before a draft pull request could be opened."
  echo
  echo "Failed stages:"
  STAGES=0
  if [ "${IMPLEMENT_RESULT}" = "failure" ]; then
    echo "- implementation (diagnostics artifact: opencode-task-issue-${ISSUE_NUMBER}-diagnostics)"
    STAGES=$((STAGES + 1))
  fi
  if [ "${VALIDATE_RESULT}" = "failure" ]; then
    echo "- validation (the implementation artifact was rejected)"
    STAGES=$((STAGES + 1))
  fi
  if [ "${PUBLISH_RESULT}" = "failure" ]; then
    echo "- publication (branch push or pull request creation)"
    STAGES=$((STAGES + 1))
  fi
  if [ "${STAGES}" -eq 0 ]; then
    echo "(none recorded; see the workflow run log)"
  fi
  echo
  if [ -n "${RUN_URL}" ]; then
    echo "Run: ${RUN_URL}"
    echo
  fi
  echo "Nothing was merged, pushed to \`${DEFAULT_BRANCH}\` or deployed. If the branch \`${BRANCH}\` was pushed, delete it to retrigger the task."
} >"${COMMENT_FILE}"

if [ "${NOTIFY_DRY_RUN}" = "1" ]; then
  echo "COMMENT_FILE=${COMMENT_FILE}"
  exit 0
fi

REPOSITORY="${REPOSITORY:?REPOSITORY is required unless NOTIFY_DRY_RUN=1}"
if gh issue comment "${ISSUE_NUMBER}" --repo "${REPOSITORY}" \
    --body-file "${COMMENT_FILE}"; then
  exit 0
fi

echo "::warning::The failure notification could not be posted. The original workflow failure remains the authoritative signal." >&2
exit 1
