#!/usr/bin/env bash
#
# Apply a validated artifact into a fresh clone, for publication.
#
# Runs in the publish job from a TRUSTED checkout. Re-runs the full
# validation (validate-changes.sh) rather than trusting the validate
# job's word, with hooks and credential helpers disabled BEFORE any Git
# operation, and prints the resulting work directory so the caller can
# push from it.
#
# Inputs (environment):
#   ARTIFACT_DIR   downloaded validated artifact
#   TRUSTED_REPO   trusted checkout of the base revision
#   BASE_SHA, BRANCH, ISSUE_NUMBER
#   WORK_DIR       disposable clone to apply into
# Outputs:
#   On success, prints "WORK=<path>" on stdout (last line).

set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

WORK_DIR="${WORK_DIR:?WORK_DIR is required}"
ARTIFACT_DIR="${ARTIFACT_DIR:?ARTIFACT_DIR is required}"
TRUSTED_REPO="${TRUSTED_REPO:?TRUSTED_REPO is required}"
BASE_SHA="${BASE_SHA:?BASE_SHA is required}"
BRANCH="${BRANCH:?BRANCH is required}"
ISSUE_NUMBER="${ISSUE_NUMBER:?ISSUE_NUMBER is required}"

# validate-changes.sh performs integrity checks, applies the patches with
# hooks disabled and inspects the resulting tree. Its exit code gates the
# publish step; a failure here must stop the caller (set -e in the job).
bash "$here/validate-changes.sh"

# Confirm the publication environment is clean before anything
# authenticated can run in the workflow.
hooks="$(git -C "${WORK_DIR}" config --get core.hooksPath || true)"
[ -n "${hooks}" ] ||
  { echo "::error::internal: hooks were not disabled during validation" >&2; exit 1; }

helper="$(git -C "${WORK_DIR}" config --get credential.helper || true)"
[ -z "${helper}" ] ||
  { echo "::error::internal: a credential helper is configured" >&2; exit 1; }

echo "WORK=${WORK_DIR}"
