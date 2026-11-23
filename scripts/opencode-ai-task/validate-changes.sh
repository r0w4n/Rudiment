#!/usr/bin/env bash
#
# Validate (and optionally re-export) an OpenCode implementation artifact.
#
# This script is security-critical and always executed from a TRUSTED
# checkout: the validate job checks out the base revision and runs this
# script from that tree; the publish job re-runs it the same way. It is
# never executed from the artifact or from agent-controlled content.
#
# What it does:
#   1. verifies the artifact's SHA256SUMS manifest,
#   2. cross-checks meta.json against the expected issue/branch/base,
#   3. scans patch text for symlinks, submodules, .git paths and traversal,
#   4. applies the patches with `git am` into a disposable clone of the
#      trusted base revision, with Git hooks and credential helpers
#      disabled, never touching the network,
#   5. inspects the resulting tree: protected paths, unexpected modes,
#      exact commit count, clean tree,
#   6. optionally re-exports a validated artifact (VALIDATED_OUT).
#
# Every failure is fail-closed: exit 1 with a ::error annotation.
#
# Inputs (environment):
#   ARTIFACT_DIR   downloaded artifact to validate (required)
#   TRUSTED_REPO   path to a checkout of the trusted base revision (required)
#   BASE_SHA       expected base commit (required)
#   BRANCH         expected task branch ai/issue-<n> (required)
#   ISSUE_NUMBER   expected issue number (required)
#   WORK_DIR       where to create the disposable clone (required)
#   VALIDATED_OUT  optional: also export a regenerated validated artifact

set -euo pipefail

. "$(dirname "$0")/lib.sh"

ARTIFACT_DIR="${ARTIFACT_DIR:?ARTIFACT_DIR is required}"
TRUSTED_REPO="${TRUSTED_REPO:?TRUSTED_REPO is required}"
BASE_SHA="${BASE_SHA:?BASE_SHA is required}"
BRANCH="${BRANCH:?BRANCH is required}"
ISSUE_NUMBER="${ISSUE_NUMBER:?ISSUE_NUMBER is required}"
WORK_DIR="${WORK_DIR:?WORK_DIR is required}"
VALIDATED_OUT="${VALIDATED_OUT:-}"

fail() { echo "::error::$1" >&2; exit 1; }

require_numeric "${ISSUE_NUMBER}" || fail "issue number is not numeric"
[ "${BRANCH}" = "ai/issue-${ISSUE_NUMBER}" ] ||
  fail "branch '${BRANCH}' does not match ai/issue-${ISSUE_NUMBER}"

# --- 1. artifact integrity -----------------------------------------------------
[ -d "${ARTIFACT_DIR}" ] || fail "artifact directory is missing"
verify_checksums "${ARTIFACT_DIR}" || fail "artifact checksum verification failed"

# --- 2. metadata cross-check ---------------------------------------------------
META="${ARTIFACT_DIR}/meta.json"
[ -f "${META}" ] || fail "meta.json missing"
META_BASE="$(json_string_value "${META}" base_sha)"
META_BRANCH="$(json_string_value "${META}" branch)"
META_ISSUE="$(json_string_value "${META}" issue_number)"
META_COUNT="$(json_number_value "${META}" commit_count)"
[ "${META_BASE}" = "${BASE_SHA}" ] || fail "artifact base_sha mismatch"
[ "${META_BRANCH}" = "${BRANCH}" ] || fail "artifact branch mismatch"
[ "${META_ISSUE}" = "${ISSUE_NUMBER}" ] || fail "artifact issue mismatch"
require_numeric "${META_COUNT}" || fail "artifact commit_count invalid"
[ -d "${ARTIFACT_DIR}/patches" ] || fail "patches directory missing"

# --- 3. patch text pre-scan ----------------------------------------------------
# Reject structures before Git ever sees them.
if grep -l -E -- '^new file mode (120000|160000)$' "${ARTIFACT_DIR}"/patches/*.patch >/dev/null 2>&1; then
  fail "patches create symlinks or submodules, which are not permitted"
fi
if grep -l -E -- '^diff --git a/\.git(/| )' "${ARTIFACT_DIR}"/patches/*.patch >/dev/null 2>&1; then
  fail "patches touch Git-internal paths"
fi
if grep -l -E -- '^\+\+\+ b/(\.\.|/)' "${ARTIFACT_DIR}"/patches/*.patch >/dev/null 2>&1; then
  fail "patches contain path traversal or absolute paths"
fi
if grep -l -E -- '^\+\+\+ b/.*\.gitmodules$' "${ARTIFACT_DIR}"/patches/*.patch >/dev/null 2>&1; then
  fail "patches add Git submodules"
fi

# --- 4. disposable clone with hooks and helpers disabled -----------------------
rm -rf "${WORK_DIR}"
git clone -q "${TRUSTED_REPO}" "${WORK_DIR}" ||
  fail "could not clone the trusted base revision"

HOOK_DIR="${WORK_DIR}.no-hooks"
mkdir -p "${HOOK_DIR}"
git -C "${WORK_DIR}" config core.hooksPath "${HOOK_DIR}"
git -C "${WORK_DIR}" config credential.helper ""
git -C "${WORK_DIR}" config user.name "opencode-ai-task[bot]"
git -C "${WORK_DIR}" config user.email "opencode-ai-task[bot]@users.noreply.github.com"

CLONED_SHA="$(git -C "${WORK_DIR}" rev-parse HEAD)"
[ "${CLONED_SHA}" = "${BASE_SHA}" ] || fail "trusted checkout is not at the expected base commit"

git -C "${WORK_DIR}" checkout -q -b "${BRANCH}" ||
  fail "could not create the task branch in the validation clone"

if ! git -C "${WORK_DIR}" am "${ARTIFACT_DIR}"/patches/*.patch >"${WORK_DIR}.am.log" 2>&1; then
  tail -n 20 "${WORK_DIR}.am.log" >&2
  rm -f "${WORK_DIR}.am.log"
  fail "patch series could not be applied cleanly"
fi
rm -f "${WORK_DIR}.am.log"

# --- 5. inspect the resulting tree --------------------------------------------
if [ -n "$(git -C "${WORK_DIR}" status --porcelain)" ]; then
  fail "applied changes left an unclean tree"
fi

TREE_DIFF="$(git -C "${WORK_DIR}" diff --name-only "${BASE_SHA}..${BRANCH}")"
if [ -z "${TREE_DIFF}" ]; then
  fail "the patch series produces no file changes"
fi

PROTECTED="$(git -C "${WORK_DIR}" diff --name-only "${BASE_SHA}..${BRANCH}" -- \
  .github/ .githooks/ scripts/opencode-ai-task/ || true)"
if [ -n "${PROTECTED}" ]; then
  echo "::error::The implementation modifies protected CI, hook or automation security paths:" >&2
  echo "${PROTECTED}" >&2
  fail "protected paths were modified"
fi

UNEXPECTED_MODES="$(git -C "${WORK_DIR}" ls-files -s | awk '$1 == "120000" || $1 == "160000" {print $4}')"
if [ -n "${UNEXPECTED_MODES}" ]; then
  echo "::error::The implementation introduces symlinks or submodules:" >&2
  echo "${UNEXPECTED_MODES}" >&2
  fail "symlinks or submodules are not permitted"
fi

APPLIED_COUNT="$(git -C "${WORK_DIR}" rev-list --count "${BASE_SHA}..${BRANCH}")"
[ "${APPLIED_COUNT}" = "${META_COUNT}" ] ||
  fail "applied commit count (${APPLIED_COUNT}) does not match the artifact (${META_COUNT})"

MERGES="$(git -C "${WORK_DIR}" rev-list --merges "${BASE_SHA}..${BRANCH}" | wc -l | tr -d ' ')"
[ "${MERGES}" -eq 0 ] || fail "merge commits appeared during validation"

# --- 6. optional validated re-export ------------------------------------------
if [ -n "${VALIDATED_OUT}" ]; then
  rm -rf "${VALIDATED_OUT}"
  mkdir -p "${VALIDATED_OUT}/patches"
  git -C "${WORK_DIR}" format-patch --binary --no-signature \
    -o "${VALIDATED_OUT}/patches" "${BASE_SHA}..${BRANCH}" >/dev/null ||
    fail "could not regenerate the validated artifact"
  cp -f "${ARTIFACT_DIR}/pr-summary.md" "${ARTIFACT_DIR}/issue-title.txt" \
    "${ARTIFACT_DIR}/meta.json" "${VALIDATED_OUT}/"
  sed 's/"validated": "false"/"validated": "true"/' "${ARTIFACT_DIR}/meta.json" \
    >"${VALIDATED_OUT}/meta.json"
  write_checksums "${VALIDATED_OUT}"
  verify_checksums "${VALIDATED_OUT}" || fail "validated artifact checksum failed"
  # The validated artifact must again contain no Git internals.
  if find "${VALIDATED_OUT}" -type d -name '.git' | grep -q .; then
    fail "validated artifact unexpectedly contains Git metadata"
  fi
fi

echo "Validation passed: ${APPLIED_COUNT} commit(s), clean tree, no protected paths, no symlinks/submodules."
