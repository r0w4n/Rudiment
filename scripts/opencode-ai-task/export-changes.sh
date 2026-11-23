#!/usr/bin/env bash
#
# Export the committed implementation changes as a restricted artifact.
#
# Runs in the implement job, AFTER the agent process has exited, from a
# trusted snapshot of this script (see the workflow's trusted-script
# snapshot step). The artifact deliberately contains only:
#   patches/     numbered `git format-patch` files (--binary)
#   meta.json    issue number, branch, base/head SHA, commit count
#   pr-summary.md, issue-title.txt
#   SHA256SUMS
# No .git metadata, hooks, configuration or credentials are ever included.
#
# Inputs (environment):
#   BRANCH           expected task branch, e.g. ai/issue-5
#   BASE_SHA         base commit the agent branched from
#   ARTIFACT_DIR     directory to write the artifact into (created fresh)
#   PR_SUMMARY_FILE  agent-written summary (optional)
#   ISSUE_TITLE_FILE one-line issue title (optional)
#
# Fails closed (exit 1) on any condition that is not exactly as expected.

set -euo pipefail

. "$(dirname "$0")/lib.sh"

BRANCH="${BRANCH:?BRANCH is required}"
BASE_SHA="${BASE_SHA:?BASE_SHA is required}"
ARTIFACT_DIR="${ARTIFACT_DIR:?ARTIFACT_DIR is required}"

fail() { echo "::error::$1" >&2; exit 1; }

# The branch name must follow the automation's own convention.
require_numeric "${BRANCH#ai/issue-}" ||
  fail "task branch '${BRANCH}' does not match ai/issue-<number>"

CURRENT_BRANCH="$(git rev-parse --abbrev-ref HEAD)"
[ "${CURRENT_BRANCH}" = "${BRANCH}" ] ||
  fail "implementation did not finish on ${BRANCH} (on '${CURRENT_BRANCH}')"

COMMIT_COUNT="$(git rev-list --count "${BASE_SHA}..${BRANCH}")"
[ "${COMMIT_COUNT}" -ge 1 ] || fail "no commits were made on ${BRANCH}"

# Merge commits cannot be represented by format-patch; require linear history.
MERGES="$(git rev-list --merges "${BASE_SHA}..${BRANCH}" | wc -l | tr -d ' ')"
[ "${MERGES}" -eq 0 ] ||
  fail "merge commits found on ${BRANCH}; linear history is required"

if [ -n "$(git status --porcelain)" ]; then
  echo "::error::OpenCode left uncommitted changes:" >&2
  git status --porcelain >&2
  fail "the working tree is not clean"
fi

# Build the artifact from scratch so no stale content can survive.
rm -rf "${ARTIFACT_DIR}"
mkdir -p "${ARTIFACT_DIR}/patches"

git format-patch --binary --no-signature \
  -o "${ARTIFACT_DIR}/patches" "${BASE_SHA}..${BRANCH}" >/dev/null ||
  fail "could not generate patch files"

PATCH_COUNT="$(find "${ARTIFACT_DIR}/patches" -type f -name '*.patch' | wc -l | tr -d ' ')"
[ "${PATCH_COUNT}" -eq "${COMMIT_COUNT}" ] ||
  fail "patch count (${PATCH_COUNT}) does not match commit count (${COMMIT_COUNT})"

if [ -n "${PR_SUMMARY_FILE:-}" ] && [ -f "${PR_SUMMARY_FILE}" ]; then
  cp -f "${PR_SUMMARY_FILE}" "${ARTIFACT_DIR}/pr-summary.md"
else
  printf '%s\n' "_(no summary produced)_" >"${ARTIFACT_DIR}/pr-summary.md"
fi

if [ -n "${ISSUE_TITLE_FILE:-}" ] && [ -f "${ISSUE_TITLE_FILE}" ]; then
  cp -f "${ISSUE_TITLE_FILE}" "${ARTIFACT_DIR}/issue-title.txt"
else
  printf '%s\n' "" >"${ARTIFACT_DIR}/issue-title.txt"
fi

HEAD_SHA="$(git rev-parse HEAD)"
cat >"${ARTIFACT_DIR}/meta.json" <<EOF
{"issue_number": "${BRANCH#ai/issue-}", "branch": "${BRANCH}", "base_sha": "${BASE_SHA}", "head_sha": "${HEAD_SHA}", "commit_count": ${COMMIT_COUNT}, "validated": "false"}
EOF

write_checksums "${ARTIFACT_DIR}"
verify_checksums "${ARTIFACT_DIR}" || fail "artifact checksum verification failed"

# Defensive sweep: the artifact must never contain Git internals.
if find "${ARTIFACT_DIR}" -type f -path '*.git*' | grep -q .; then
  fail "artifact unexpectedly contains Git metadata"
fi

echo "Exported ${COMMIT_COUNT} commit(s) to ${ARTIFACT_DIR}:"
find "${ARTIFACT_DIR}" -type f | LC_ALL=C sort
