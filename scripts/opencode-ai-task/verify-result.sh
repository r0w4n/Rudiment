#!/usr/bin/env bash
#
# Verify and guard the result of an OpenCode implementation run before the
# automation pushes anything. Behavioural tests live in
# scripts/opencode-ai-task/test-verify-result.sh.
#
# Inputs (environment):
#   BRANCH           The dedicated branch OpenCode was instructed to use.
#   DEFAULT_BRANCH   The repository's default branch.
#   ISSUE_NUMBER     The GitHub issue number (for the step summary heading).
#   GITHUB_STEP_SUMMARY  Path to append a Markdown summary to (optional in tests).
#
# Exits 0 only when it is safe to push BRANCH:
#   - the current branch is BRANCH,
#   - BRANCH is not the default branch,
#   - the branch has at least one commit beyond the default branch,
#   - the working tree is clean,
#   - no changes under .github/ or .githooks/ (CI and security configuration).

set -euo pipefail

BRANCH="${BRANCH:?BRANCH is required}"
DEFAULT_BRANCH="${DEFAULT_BRANCH:?DEFAULT_BRANCH is required}"
ISSUE_NUMBER="${ISSUE_NUMBER:-}"

fail() {
  echo "::error::$1" >&2
  exit 1
}

# Defensive check: the automation must never target the default branch itself.
if [ "${BRANCH}" = "${DEFAULT_BRANCH}" ]; then
  fail "Task branch must never be the default branch (${DEFAULT_BRANCH})."
fi

CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
if [ "${CURRENT_BRANCH}" != "${BRANCH}" ]; then
  fail "OpenCode did not create or switch to branch ${BRANCH} (still on '${CURRENT_BRANCH}')."
fi

COMMIT_COUNT="$(git rev-list --count "origin/${DEFAULT_BRANCH}..HEAD")"
if [ "${COMMIT_COUNT}" -lt 1 ]; then
  fail "No commits were made on ${BRANCH}."
fi

if [ -n "$(git status --porcelain)" ]; then
  echo "::error::OpenCode left uncommitted changes:" >&2
  git status --porcelain >&2
  fail "The working tree is not clean."
fi

FORBIDDEN="$(git diff --name-only "origin/${DEFAULT_BRANCH}...HEAD" -- .github/ .githooks/ || true)"
if [ -n "${FORBIDDEN}" ]; then
  echo "::error::The implementation modified protected CI or security paths. Pushing is blocked pending manual approval:" >&2
  echo "${FORBIDDEN}" >&2
  fail "Changes to CI or security configuration are not permitted."
fi

if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
  {
    echo "## OpenCode AI task — issue #${ISSUE_NUMBER}"
    echo
    echo "Branch: \`${BRANCH}\` — commits:"
    echo
    git log --oneline "origin/${DEFAULT_BRANCH}..HEAD"
    echo
    echo "Changed files:"
    echo
    echo '```'
    git diff --name-only "origin/${DEFAULT_BRANCH}...HEAD"
    echo '```'
  } >>"${GITHUB_STEP_SUMMARY}"
fi

echo "Verification passed: ${COMMIT_COUNT} commit(s) on ${BRANCH}, clean tree, no protected-path changes."
