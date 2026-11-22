#!/usr/bin/env bash
#
# Authorisation gate for the OpenCode AI task workflow.
#
# Decides whether an issue-triggered event may start an OpenCode coding task.
# The script is pure: it reads only the environment variables listed below and
# performs no network access, so it can be unit-tested locally with mocked
# event payloads.
#
# Inputs (environment):
#   GATE_EVENT_LABEL      Label name that was applied (issues:labeled events).
#   GATE_ISSUE_STATE      Issue state: "open" or "closed".
#   GATE_ISSUE_AUTHOR     Login of the user who created the issue.
#   GATE_SENDER           Login of the user who applied the label / triggered.
#   GATE_IS_PULL_REQUEST  "true" when the issue record belongs to a pull request.
#   GATE_BODY_NONEMPTY    "true" when the issue description is non-empty.
#   GATE_REQUIRED_AUTHOR  Login allowed to create issues and apply the label.
#   GATE_REQUIRED_LABEL   Label that must be applied.
#
# Output:
#   A single line "proceed=<true|false> reason=<text>".
#   Exit code 0 for both allow and deny (policy outcomes); exit code 1 only for
#   configuration errors so the calling workflow can surface them as failures.
#
# Comparison is case-insensitive for logins to match GitHub's semantics.

set -euo pipefail

required_author="${GATE_REQUIRED_AUTHOR:?GATE_REQUIRED_AUTHOR is required}"
required_label="${GATE_REQUIRED_LABEL:?GATE_REQUIRED_LABEL is required}"

deny() {
  echo "proceed=false reason=$1"
  exit 0
}

allow() {
  echo "proceed=true reason=$1"
  exit 0
}

deny_because() {
  echo "denied: $1" >&2
  deny "$1"
}

event_label="${GATE_EVENT_LABEL:-}"
issue_state="${GATE_ISSUE_STATE:-}"
issue_author="${GATE_ISSUE_AUTHOR:-}"
sender="${GATE_SENDER:-}"
is_pull_request="${GATE_IS_PULL_REQUEST:-false}"
body_nonempty="${GATE_BODY_NONEMPTY:-false}"

if [ -z "$issue_author" ] || [ -z "$sender" ]; then
  echo "denied: GATE_ISSUE_AUTHOR and GATE_SENDER must be set" >&2
  exit 1
fi

# Never allow pull-request issues: AI tasks are created from development issues.
if [ "$is_pull_request" = "true" ]; then
  deny_because "event belongs to a pull request, not a development issue"
fi

# Only the configured label may trigger a task.
if [ "$event_label" != "$required_label" ]; then
  deny_because "label '${event_label:-<none>}' is not '${required_label}'"
fi

# The issue itself must have been created by the authorised user.
lowercase() { printf '%s' "$1" | tr '[:upper:]' '[:lower:]'; }

if [ "$(lowercase "$issue_author")" != "$(lowercase "$required_author")" ]; then
  deny_because "issue author '${issue_author}' is not '${required_author}'"
fi

# The user applying the label must be the authorised user as well.
if [ "$(lowercase "$sender")" != "$(lowercase "$required_author")" ]; then
  deny_because "label applied by '${sender}', not '${required_author}'"
fi

# Closed issues never start new work.
if [ "$issue_state" != "open" ]; then
  deny_because "issue is ${issue_state:-<unknown>}"
fi

# An empty description carries no specification to implement.
if [ "$body_nonempty" != "true" ]; then
  deny_because "issue description is empty"
fi

allow "issue #authorised: author, labeler and label verified"
