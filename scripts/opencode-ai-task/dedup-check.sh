#!/usr/bin/env bash
#
# Duplicate prevention for the OpenCode AI task workflow.
#
# Checks the remote state for the task branch of one issue:
#   exit 0 -> no branch exists; the task may proceed
#   exit 3 -> branch and an open pull request already exist (skip quietly)
#   exit 2 -> branch exists without an open PR (previous run incomplete)
#   exit 1 -> configuration or query error
#
# Uses the `gh` CLI (already authenticated by the caller). Read-only.

set -euo pipefail

. "$(dirname "$0")/lib.sh"

ISSUE_NUMBER="${1:?usage: dedup-check.sh <issue-number>}"
REPO="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is required}"

require_numeric "$ISSUE_NUMBER" || { echo "invalid issue number" >&2; exit 1; }

BRANCH="ai/issue-${ISSUE_NUMBER}"

if gh api "repos/${REPO}/branches/${BRANCH}" >/dev/null 2>&1; then
  OPEN_PRS="$(gh pr list --repo "${REPO}" --head "${BRANCH}" --state open --json number --jq 'length')"
  if [ "${OPEN_PRS}" -gt 0 ]; then
    echo "::notice::Issue #${ISSUE_NUMBER} already has an active task: branch ${BRANCH} and an open pull request exist. Nothing to do."
    exit 3
  fi
  echo "::error::Branch ${BRANCH} exists without an open pull request: a previous run did not complete. Delete the branch to retrigger: git push origin --delete ${BRANCH}"
  exit 2
fi

exit 0
