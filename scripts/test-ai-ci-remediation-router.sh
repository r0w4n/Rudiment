#!/usr/bin/env bash
#
# Behavioural tests for the AI CI remediation router in
# .github/workflows/ai-ci-remediation.yml. The evaluate step's script is
# extracted from the workflow and executed against a deterministic gh stub
# with realistic GitHub event payloads and API responses, covering dispatch,
# skips, rejections and explicit API-error diagnostics.
#
# "Duplicate events do not start competing agents" is covered here by the
# router's serialising concurrency group (asserted statically in
# scripts/test-ai-task-environment.sh) and by Central's in-progress episode
# deduplication (scripts/opencode-ai-task/test-remediation.sh upstream).

set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
router="$root/.github/workflows/ai-ci-remediation.yml"

pass=0
fail=0
ok() { pass=$((pass + 1)); echo "PASS: $1"; }
ko() { fail=$((fail + 1)); echo "FAIL: $1"; }
check() { if eval "$2"; then ok "$1"; else ko "$1"; fi; }

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

# --- Extract the evaluate step script from the workflow -----------------------
SCRIPT="$tmp/router-script.sh"
awk '
  /^      - name: Evaluate checks and dispatch the AI task workflow$/ {step=1; next}
  step && !run && /^        run: \|/ {run=1; next}
  step && run && /^        [^ ]/ {exit}
  step && run {print substr($0, 11)}
' "$router" >"$SCRIPT"
[ -s "$SCRIPT" ] || { echo "FAIL: could not extract the router script"; exit 1; }
bash -n "$SCRIPT" || { echo "FAIL: extracted router script has a syntax error"; exit 1; }
ok "router script extracts cleanly from the workflow"

# --- Fixtures -------------------------------------------------------------------
# Modelled on Rudiment PR #48 (ai/issue-37 by central-ai-publisher[bot]).
SHA=9982c96715bbdab465355600d050ddb73c68c2ce
NEW_SHA=1111111111111111111111111111111111111111
PR48="$(cat <<EOF
{"number":48,"state":"open","user":{"login":"central-ai-publisher[bot]"},"base":{"ref":"main"},"head":{"ref":"ai/issue-37","sha":"${SHA}","repo":{"full_name":"r0w4n/Rudiment"}}}
EOF
)"
PR48_HUMAN="$(jq -c '.user.login = "r0w4n"' <<<"$PR48")"
PR48_FORK="$(jq -c '.head.repo.full_name = "mallory/fork"' <<<"$PR48")"
PR48_MOVED="$(jq -c --arg s "$NEW_SHA" '.head.sha = $s' <<<"$PR48")"
CHECKS_FAILED='{"total_count":2,"check_runs":[{"id":1,"name":"Quality assurance","status":"completed","conclusion":"failure","details_url":"https://example.invalid/1"},{"id":2,"name":"CodeQL static security analysis","status":"completed","conclusion":"success","details_url":""}]}'
CHECKS_GREEN='{"total_count":2,"check_runs":[{"id":1,"name":"Quality assurance","status":"completed","conclusion":"success","details_url":""},{"id":2,"name":"CodeQL static security analysis","status":"completed","conclusion":"success","details_url":""}]}'
CHECKS_PENDING='{"total_count":2,"check_runs":[{"id":1,"name":"Quality assurance","status":"completed","conclusion":"failure","details_url":""},{"id":2,"name":"Performance regression checks","status":"in_progress","conclusion":null,"details_url":""}]}'
DISPATCH_BODY_FILE="$tmp/dispatch-body.json"
CALL_LOG="$tmp/gh-calls.log"
export DISPATCH_BODY_FILE CALL_LOG

# --- gh stub --------------------------------------------------------------------
# Emulates 'gh api [--paginate] <url> -f k=v ... --jq <expr>' by applying the
# given jq expression to the fixture response of the matched URL. Fixtures
# are passed through the environment so the stub script itself stays static.
make_stub() { # $1 = pulls-list JSON, $2 = pulls/48 JSON, $3 = check-runs response (JSON or "FAIL")
  ROUTER_PULLS_FIXTURE="$1"
  ROUTER_PR_FIXTURE="$2"
  ROUTER_CHECKS_FIXTURE="$3"
  export ROUTER_PULLS_FIXTURE ROUTER_PR_FIXTURE ROUTER_CHECKS_FIXTURE
  cat >"$tmp/bin/gh" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALL_LOG"
jq_expr=""
prev=""
for arg in "$@"; do
  [ "${prev}" = "--jq" ] && jq_expr="${arg}"
  prev="${arg}"
done
respond() { # $1 = response text, $2 = jq expr (may be empty)
  if [ -n "${2:-}" ]; then printf '%s' "$1" | jq -r "$2"; else printf '%s' "$1"; fi
}
case "$*" in
  *"actions/workflows/opencode-ai-task.yml/dispatches"*)
    cat >"$DISPATCH_BODY_FILE"
    ;;
  *"check-runs"*)
    if [ "${ROUTER_CHECKS_FIXTURE}" = "FAIL" ]; then
      echo 'HTTP 403: checks scope missing' >&2
      exit 1
    fi
    respond "${ROUTER_CHECKS_FIXTURE}" "${jq_expr:-}"
    ;;
  *"repos/r0w4n/Rudiment/pulls/48"*)
    respond "${ROUTER_PR_FIXTURE}" "${jq_expr:-}"
    ;;
  *"repos/r0w4n/Rudiment/pulls"*)
    respond "${ROUTER_PULLS_FIXTURE}" "${jq_expr:-}"
    ;;
  *) echo "unexpected gh call: $*" >&2; exit 1 ;;
esac
STUB
  chmod +x "$tmp/bin/gh"
}

run_router() { # extra env assignments as arguments
  env \
    REPOSITORY=r0w4n/Rudiment \
    AUTOMATION_PR_AUTHOR='central-ai-publisher[bot]' \
    APPROVED_BASE_BRANCH=main \
    AI_TASK_WORKFLOW=opencode-ai-task.yml \
    REMEDIATION_MAX_ATTEMPTS=3 \
    QUIESCE_TIMEOUT_SECONDS=0 \
    QUIESCE_POLL_SECONDS=60 \
    DRY_RUN= \
    "$@" \
    PATH="$tmp/bin:$PATH" \
    bash "$SCRIPT"
}

expect_no_dispatch() {
  [ ! -s "$DISPATCH_BODY_FILE" ] || ko "unexpected dispatch body: $(cat "$DISPATCH_BODY_FILE")"
  ! grep -q 'dispatches' "$CALL_LOG" 2>/dev/null
}

mkdir -p "$tmp/bin"

# --- 1. Failed check on an authorised AI PR dispatches remediation --------------
make_stub "[${PR48}]" "$PR48" "$CHECKS_FAILED"
rm -f "$DISPATCH_BODY_FILE" "$CALL_LOG"
OUT="$(run_router \
  EVENT_NAME=workflow_run \
  TRIGGER_CONCLUSION=failure \
  TRIGGER_HEAD_BRANCH=ai/issue-37 \
  TRIGGER_HEAD_SHA="$SHA" \
  TRIGGER_RUN_ID=46150276705 \
  REMEDIATION_RUN_ID=46150276705 \
  DRY_RUN= RUN_URL=https://example.invalid/router 2>&1)"
check "a failed check dispatches remediation without a comment" \
  "grep -q 'Dispatched checks-failed remediation for PR #48' <<<\"\$OUT\""
check "the failing check is named in the log" "grep -q 'Quality assurance' <<<\"\$OUT\""
check "the dispatch targets the caller workflow" "grep -q 'actions/workflows/opencode-ai-task.yml/dispatches' '$CALL_LOG'"
DISPATCH_MODE="$(jq -r '.inputs.task_mode' "$DISPATCH_BODY_FILE")"
check "the dispatch runs in remediation mode on the approved base" \
  "[ \"\$(jq -r '.ref' '$DISPATCH_BODY_FILE')\" = main ] && [ \"\$DISPATCH_MODE\" = remediation ]"
check "the dispatch carries the PR number" "[ \"\$(jq -r '.inputs.pr_number' '$DISPATCH_BODY_FILE')\" = 48 ]"
check "the dispatch carries the checks-failed event" \
  "[ \"\$(jq -r '.inputs.remediation_event' '$DISPATCH_BODY_FILE')\" = checks-failed ]"
check "the dispatch carries the attempt limit" \
  "[ \"\$(jq -r '.inputs.remediation_max_attempts' '$DISPATCH_BODY_FILE')\" = 3 ]"
check "the triggering run id is forwarded through remediation_run_id" \
  "[ \"\$(jq -r '.inputs.remediation_run_id' '$DISPATCH_BODY_FILE')\" = 46150276705 ]"
check "the dispatch is not a dry run" "[ \"\$(jq -r '.inputs.dry_run' '$DISPATCH_BODY_FILE')\" = false ]"
check "no PR comment is posted" "! grep -q 'issues/48/comments' '$CALL_LOG'"

# --- 2. Green checks close the episode ------------------------------------------
make_stub "[${PR48}]" "$PR48" "$CHECKS_GREEN"
rm -f "$DISPATCH_BODY_FILE" "$CALL_LOG"
OUT="$(run_router \
  EVENT_NAME=workflow_run \
  TRIGGER_CONCLUSION=success \
  TRIGGER_HEAD_BRANCH=ai/issue-37 \
  TRIGGER_HEAD_SHA="$SHA" \
  TRIGGER_RUN_ID=46150276636 \
  REMEDIATION_RUN_ID=46150276636 2>&1)"
check "green checks dispatch checks-passed" \
  "grep -q 'Dispatched checks-passed remediation for PR #48' <<<\"\$OUT\""
check "the checks-passed dispatch is recorded" \
  "[ \"\$(jq -r '.inputs.remediation_event' '$DISPATCH_BODY_FILE')\" = checks-passed ]"

# --- 3. Human-created PRs, forks and foreign branches stay excluded --------------
make_stub "[${PR48_HUMAN}]" "$PR48_HUMAN" "$CHECKS_FAILED"
rm -f "$DISPATCH_BODY_FILE" "$CALL_LOG"
OUT="$(run_router \
  EVENT_NAME=workflow_run TRIGGER_CONCLUSION=failure \
  TRIGGER_HEAD_BRANCH=ai/issue-37 TRIGGER_HEAD_SHA="$SHA" TRIGGER_RUN_ID=1 2>&1)"
check "a human-created PR is not remediated" "grep -q 'not an authorised AI pull request' <<<\"\$OUT\""
expect_no_dispatch && ok "a human-created PR triggers no dispatch" || ko "a human-created PR triggers no dispatch"

make_stub "[${PR48_FORK}]" "$PR48_FORK" "$CHECKS_FAILED"
rm -f "$DISPATCH_BODY_FILE" "$CALL_LOG"
OUT="$(run_router \
  EVENT_NAME=workflow_run TRIGGER_CONCLUSION=failure \
  TRIGGER_HEAD_BRANCH=ai/issue-37 TRIGGER_HEAD_SHA="$SHA" TRIGGER_RUN_ID=1 2>&1)"
expect_no_dispatch && ok "a forked PR head triggers no dispatch" || ko "a forked PR head triggers no dispatch"

make_stub "[${PR48}]" "$PR48" "$CHECKS_FAILED"
rm -f "$DISPATCH_BODY_FILE" "$CALL_LOG"
OUT="$(run_router \
  EVENT_NAME=workflow_run TRIGGER_CONCLUSION=failure \
  TRIGGER_HEAD_BRANCH=renovate/lockfile TRIGGER_HEAD_SHA="$SHA" TRIGGER_RUN_ID=1 2>&1)"
check "a non-AI branch exits immediately" "grep -q 'not an AI task branch' <<<\"\$OUT\""
expect_no_dispatch && ok "a non-AI branch triggers no dispatch" || ko "a non-AI branch triggers no dispatch"

# --- 4. API failures produce explicit diagnostics, never silent no-ops ----------
make_stub "[]" "$PR48" "$CHECKS_FAILED"
cat >"$tmp/bin/gh" <<'STUB'
#!/usr/bin/env bash
printf '%s\n' "$*" >>"$CALL_LOG"
case "$*" in
  *"repos/r0w4n/Rudiment/pulls"*) echo 'HTTP 502' >&2; exit 1 ;;
  *) echo 'unexpected call' >&2; exit 1 ;;
esac
STUB
chmod +x "$tmp/bin/gh"
rm -f "$DISPATCH_BODY_FILE" "$CALL_LOG"
if OUT="$(run_router \
  EVENT_NAME=workflow_run TRIGGER_CONCLUSION=failure \
  TRIGGER_HEAD_BRANCH=ai/issue-37 TRIGGER_HEAD_SHA="$SHA" TRIGGER_RUN_ID=1 2>&1)"; then
  ko "a PR-list API failure fails the evaluation loudly"
else
  ok "a PR-list API failure fails the evaluation loudly"
fi
check "the PR-list failure names the failed request" "grep -q '::error::.*PR lookup' <<<\"\$OUT\""
expect_no_dispatch && ok "a failed PR lookup dispatches nothing" || ko "a failed PR lookup dispatches nothing"

make_stub "[${PR48}]" "$PR48" "FAIL"
rm -f "$DISPATCH_BODY_FILE" "$CALL_LOG"
if OUT="$(run_router \
  EVENT_NAME=workflow_run TRIGGER_CONCLUSION=failure \
  TRIGGER_HEAD_BRANCH=ai/issue-37 TRIGGER_HEAD_SHA="$SHA" TRIGGER_RUN_ID=1 2>&1)"; then
  ko "a check-run API failure fails the evaluation loudly"
else
  ok "a check-run API failure fails the evaluation loudly"
fi
check "the check-run failure names the missing scope" "grep -q 'checks: read' <<<\"\$OUT\""
expect_no_dispatch && ok "a failed check-run lookup dispatches nothing" || ko "a failed check-run lookup dispatches nothing"

# An empty authorised-PR result is a genuine skip, not an error.
make_stub "[]" "$PR48" "$CHECKS_FAILED"
rm -f "$DISPATCH_BODY_FILE" "$CALL_LOG"
OUT="$(run_router \
  EVENT_NAME=workflow_run TRIGGER_CONCLUSION=failure \
  TRIGGER_HEAD_BRANCH=ai/issue-37 TRIGGER_HEAD_SHA="$SHA" TRIGGER_RUN_ID=1 2>&1)"
check "an empty PR lookup result skips cleanly" "grep -q 'No open authorised AI PR' <<<\"\$OUT\""
expect_no_dispatch && ok "an empty PR lookup result dispatches nothing" || ko "an empty PR lookup result dispatches nothing"

# --- 5. Stale events are dropped -------------------------------------------------
make_stub "[${PR48_MOVED}]" "$PR48_MOVED" "$CHECKS_FAILED"
rm -f "$DISPATCH_BODY_FILE" "$CALL_LOG"
OUT="$(run_router \
  EVENT_NAME=workflow_run TRIGGER_CONCLUSION=failure \
  TRIGGER_HEAD_BRANCH=ai/issue-37 TRIGGER_HEAD_SHA="$SHA" TRIGGER_RUN_ID=1 2>&1)"
check "an event for a superseded head is dropped" "grep -q 'not the PR head' <<<\"\$OUT\""
expect_no_dispatch && ok "a superseded head dispatches nothing" || ko "a superseded head dispatches nothing"

# Pending checks outlive the (zero-length) quiesce budget; evaluation still
# runs on the current results and the timeout is reported.
make_stub "[${PR48}]" "$PR48" "$CHECKS_PENDING"
rm -f "$DISPATCH_BODY_FILE" "$CALL_LOG"
OUT="$(run_router \
  EVENT_NAME=workflow_run TRIGGER_CONCLUSION=failure \
  TRIGGER_HEAD_BRANCH=ai/issue-37 TRIGGER_HEAD_SHA="$SHA" TRIGGER_RUN_ID=1 2>&1)"
check "a quiesce timeout is reported and evaluation continues" \
  "grep -q 'did not quiesce' <<<\"\$OUT\""
check "a verified failure amid pending checks still remediates" \
  "grep -q 'Dispatched checks-failed remediation for PR #48' <<<\"\$OUT\""

# --- 6. Manual recovery of an existing failed AI PR ------------------------------
make_stub "[${PR48}]" "$PR48" "$CHECKS_FAILED"
rm -f "$DISPATCH_BODY_FILE" "$CALL_LOG"
OUT="$(run_router \
  EVENT_NAME=workflow_dispatch \
  INPUT_PR_NUMBER=48 DRY_RUN=false TRIGGER_RUN_ID= REMEDIATION_RUN_ID= 2>&1)"
check "manual recovery evaluates an existing AI PR" "grep -q 'Recovering AI PR #48' <<<\"\$OUT\""
check "manual recovery dispatches remediation" \
  "grep -q 'Dispatched checks-failed remediation for PR #48' <<<\"\$OUT\""
check "manual recovery is not a dry run" "[ \"\$(jq -r '.inputs.dry_run' '$DISPATCH_BODY_FILE')\" = false ]"

make_stub "[${PR48_HUMAN}]" "$PR48_HUMAN" "$CHECKS_FAILED"
rm -f "$DISPATCH_BODY_FILE" "$CALL_LOG"
OUT="$(run_router \
  EVENT_NAME=workflow_dispatch \
  INPUT_PR_NUMBER=48 DRY_RUN=false 2>&1)"
check "manual recovery skips a non-AI PR" "grep -q 'not an open AI pull request' <<<\"\$OUT\""
expect_no_dispatch && ok "manual recovery of a non-AI PR dispatches nothing" || ko "manual recovery of a non-AI PR dispatches nothing"

# --- 7. Dry run evaluates without dispatching ------------------------------------
make_stub "[${PR48}]" "$PR48" "$CHECKS_FAILED"
rm -f "$DISPATCH_BODY_FILE" "$CALL_LOG"
OUT="$(run_router \
  EVENT_NAME=workflow_dispatch \
  INPUT_PR_NUMBER=48 DRY_RUN=true 2>&1)"
check "a dry run announces the dispatch it would make" "grep -q 'Dry run: would dispatch' <<<\"\$OUT\""
expect_no_dispatch && ok "a dry run dispatches nothing" || ko "a dry run dispatches nothing"

echo
echo "Results: $pass passed, $fail failed"
[ "$fail" -eq 0 ]
