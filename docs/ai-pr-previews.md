# AI pull request reviews: checks and isolated previews

This repository runs an AI-assisted development flow: an authorised issue
labelled `ai-task` is implemented by OpenCode through the reusable workflow in
`r0w4n/central-ai-workflow` (pinned by commit SHA in
`.github/workflows/opencode-ai-task.yml`), published as a ready-for-review
pull request, checked automatically by the existing repository workflows, and
previewed in the browser through an isolated GitHub Pages deployment.
Authorised review feedback can be sent back to OpenCode on that same PR; the
resulting commit reruns the same checks and replaces the existing preview.

Merging stays a manual action in every case. Nothing in this flow pushes to
`main`, and the production GitHub Pages deployment
(`.github/workflows/pages.yml`) is never touched by previews.

## What runs automatically on an AI pull request

1. `.github/workflows/opencode-ai-task.yml` turns the labelled issue into a
   `ai/issue-<n>` branch and opens a **ready-for-review** PR (the caller sets
   `draft: false`). The publication stage uses GitHub App credentials, so the
   PR triggers this repository's own `pull_request` workflows without
   GitHub's held-for-approval gate.
2. The existing PR workflows run automatically with no manual approval:
   `quality.yml`, `performance.yml`, `codeql.yml`, and `security.yml` when its
   dependency paths match, plus `.github/workflows/ai-pr-preview.yml`.
3. `.github/workflows/ai-pr-preview.yml` builds the exact PR commit with the
   production build and relative asset paths, publishes the bundle to the
   preview repository's Pages site at
   `https://r0w4n.github.io/Rudiment-previews/pr-<number>/`, and adds a single
   sticky "Rudiment preview" comment with the URL. New commits replace the
   same preview URL; closing or merging the PR removes the preview and retires
   the comment.
4. Required status checks (see "Branch protection" below) block merging while
   any check fails.

Human-created pull requests keep their normal behaviour; the preview workflow
only acts on same-repository branches named `ai/*`.

## Requesting an AI revision

On an open PR created by `central-ai-publisher[bot]` from an `ai/issue-<n>`
branch, `r0w4n` can request changes either with the explicit command:

```text
/ai revise Describe the focused correction to make.
```

or by writing an ordinary actionable comment describing the change. Commands
no longer need to be remembered: the lightweight Rudiment caller forwards all
PR comments on AI pull requests to the generic Central AI workflow, which
classifies each comment locally. Explicit `/ai revise` commands work exactly
as before; ordinary actionable feedback starts a revision the same way;
malformed `/ai` commands (such as `/ai assist`) receive a public reply with
the valid usage instead of being silently skipped; vague directives and bare
questions receive an acknowledgement asking for a concrete change; bot
comments, automation status updates, pleasantries and unauthorised feedback
are ignored silently.

Central revalidates the human, PR creator, same-repository head, `main` base,
linked authorised issue, immutable comment ID, open state, and exact current
head. OpenCode starts from the current PR implementation and commits only the
requested correction to the existing branch; it does not create another issue
or PR.

Requests for the same PR are queued with cancellation disabled. Each feedback
comment has one audit comment that moves from accepted to completed or failed
and links the workflow run and resulting commit. Closed or merged PRs, forks,
unexpected branches, and unauthorised users cannot start an agent. Feedback is
untrusted task data and cannot change workflow files, secrets, protected paths,
permissions, model selection, or publication rules.

The revision push uses the GitHub App, so GitHub emits the normal
`pull_request/synchronize` event. `quality.yml`, `performance.yml`,
`codeql.yml`, applicable `security.yml` jobs, and `ai-pr-preview.yml` rerun.
The preview workflow publishes to the existing `pr-<number>/` directory and
updates its sticky comment; it never touches production Pages.

To retry a failed run, manually dispatch **OpenCode AI task**, select
`revision`, and enter the existing PR number plus the numeric ID of the
original feedback comment (visible in the comment permalink). Leave `dry_run`
enabled to recheck policy without writing a status comment or running
OpenCode; disable it for the actual retry. GitHub's **Re-run jobs** action is
also supported. Never repost identical feedback merely to retry.

## Automatic CI-failure remediation

The `ai-ci-remediation.yml` router closes the CI loop without manual `/ai
revise` comments. It listens for `workflow_run: completed` events of
`quality.yml`, `performance.yml`, `codeql.yml` and `security.yml`, and:

1. ignores human-created PRs, forks and non-`ai/issue-<n>` branches outright;
2. waits (bounded) for concurrently running checks on the same head to
   quiesce, so one remediation sees every failure of that commit —
   path-filtered workflows that do not run simply never appear and are not
   waited for;
3. forwards the outcome to the pinned Central workflow in `remediation`
   mode (failed checks, or all-green to close an episode and notify).

Central owns all remediation behaviour: it revalidates the AI pull request
from current API state, reads the failing checks of the current head, supplies
their names and redacted log excerpts together with the original issue to
OpenCode, and pushes the correction to the same branch under the same
head-SHA lease as revisions. CI and previews then rerun automatically.

A single persistent status comment per PR tracks the episode: current status,
failing checks, attempt number and maximum (default three per failure
episode), the corrective summary, workflow links and the final outcome. Failed
attempts dispatch bounded automatic retries; manual revisions and automatic
remediation share one per-PR queue, so competing agents are impossible. When
the attempts are exhausted, Central escalates with actionable diagnostics and
stops — human intervention is requested on the PR. Nothing is ever
auto-approved or merged, and the owner is notified only when the PR is ready
for review or needs intervention.

AI pull requests that failed before this router existed are recovered with the
router's manual dispatch: run **AI CI remediation router** with the PR number
(`dry_run` first to preview the decision). The same authorisation applies.

## AI task environment ownership

Rudiment owns the environment used by the implementation agent. The central
caller opts into `.github/actions/setup-ai-task`, which delegates to the same
`.github/actions/setup-rudiment` action used by repository CI. It installs
Node 22, the Rust toolchain pinned in `rust-toolchain.toml`, the WASM target,
Trunk 0.22.1 and locked npm dependencies, then verifies their versions. It
does not compile the application before OpenCode starts; the agent runs the
relevant checks for its change and the pull-request workflows remain the
authoritative quality gates.

The setup summary records exact npm, Rust and Trunk cache hits, npm install
duration and total environment preparation time. Rust cache identity includes
the dependency manifests and lockfile, pinned toolchain, compilation target and
Trunk version; npm and Trunk caches include their lock/version/checksum inputs.
Only dependency downloads, compiled dependencies and the verified tool binary
are cached—never credentials, model sessions or agent state.

The AI implementation job restores compatible caches but disables every cache
save. This matters because the setup action and OpenCode share a runner:
post-job cache saves would otherwise be able to persist files after the agent
had modified them. Ordinary CI saves npm and Trunk caches immediately during
setup, and Rust post-job saves remain enabled only in repository validation
jobs. Untrusted pull requests remain confined to GitHub's branch cache scope.

## Preview isolation model

- The `build` job checks out the PR commit and executes untrusted PR code
  with **no secrets and read-only permissions**. Its only output is the
  `dist/` bundle artifact, which the publish stage treats strictly as data
  (symlink, embedded-Git-directory and absolute-path guards run before use).
- The `publish` and `cleanup` jobs run no PR code. They mint a short-lived
  GitHub App installation token scoped to `r0w4n/Rudiment-previews` contents
  only, and push the bundle under `pr-<number>/` on that repository's
  `gh-pages` branch. Other previews live side by side under their own paths.
- Previews can never overwrite production: they are plain files in a
  different repository's Pages site, while production Rudiment is deployed
  from `main` via `actions/deploy-pages` in `.github/workflows/pages.yml`.

## One-time configuration

These steps require the repository owner in the GitHub UI; they cannot be
performed by workflow YAML.

1. **Create the GitHub App** (one App serves both the AI task publication and
   the preview deployment):
   - Settings > Developer settings > GitHub Apps > New GitHub App, e.g.
     `rudiment-automation`. Homepage URL can be this repository.
   - Grant repository permissions: **Contents: Read and write**, **Pull
     requests: Read and write**. No account permissions, no webhooks.
   - Note the **App ID** and download the private key (PKCS#8 PEM).
2. **Install the App** on `r0w4n/Rudiment` and `r0w4n/Rudiment-previews`.
3. **Create the preview repository** `r0w4n/Rudiment-previews` (public). The
   preview workflow creates the `gh-pages` branch automatically on first
   deployment; alternatively push an orphan `gh-pages` branch with a
   `.nojekyll` file. Then enable **Settings > Pages > Build and deployment >
   Deploy from a branch: `gh-pages` / root**.
4. **Add Actions secrets** to `r0w4n/Rudiment`:
   - `OPENCODE_GO_API_KEY` (already present).
   - `APP_ID` — the GitHub App ID from step 1.
   - `APP_PRIVATE_KEY` — the contents of the downloaded private key.
5. **Repository settings for GITHUB_TOKEN fallback** (only needed if the App
   secrets are ever removed): Settings > Actions > General > Workflow
   permissions > *Read and write permissions* and *Allow GitHub Actions to
   create and approve pull requests*. The reusable workflow's authorisation
   stage preflights both settings and fails with remediation before an
   expensive implementation when they are missing.

### Branch protection

The `main required status checks` ruleset (Settings > Rules > Rulesets) is
active on `main`:

- **Require status checks to pass** before the branch is updated:
  `Code quality, unit tests, and build`, `Browser correctness and
  accessibility`, `fast` (performance), `CodeQL (javascript-typescript)`, and
  `CodeQL (rust)`. `security.yml` keeps its path-based trigger and is not a
  required check, because path-filtered workflows never report on PRs outside
  their paths.
- The repository administrator is listed as a bypass actor, so the
  established local merge-and-push integration flow keeps working; GitHub
  logs every bypass and the merge UI states it explicitly. The automation
  itself has no bypass: a failing required check blocks any automated merge,
  and nothing in the automation merges (no `gh pr merge` anywhere).

## Failure modes

- **Preview deployment failed** appears as the sticky PR comment with a link
  to the workflow run logs whenever the build or publish stage fails; the
  build job itself is surfaced through the normal PR check.
- Missing or misconfigured App credentials fail the preview publish job with
  an explicit remediation message instead of a generic Git error.
- A failed PR creation in the AI task workflow deletes the pushed branch so
  no orphan `ai/issue-<n>` branch remains.
- A failed revision updates its audit comment with the workflow run and manual
  retry inputs. If the PR head changed while OpenCode was working, publication
  fails rather than overwriting the newer commit.

## Validated end to end

Validated against a temporary same-repository `ai/*` pull request (opened,
updated and closed without merging):

- All repository PR checks (`quality`, `performance`, `codeql`) started
  automatically with no approval gate and reported on the PR.
- The preview bundle built from the PR commit with the production relative
  asset paths; index, bridge script, CSS and WASM all served over the
  `pr-<number>/` subpath.
- A new push to the PR re-triggered the preview build for the new commit;
  closing the PR cancelled an in-flight build and triggered cleanup.
- Publishing and removing the preview directory on the `gh-pages` branch
  (the exact operations the publish and cleanup stages run) were verified
  against the live Pages site, including the 404 after removal, the intact
  root page, and an unchanged production site.
- Missing App credentials fail the publish and cleanup stages with the
  documented remediation message; the sticky PR comment reports the failure
  with the run link. The App-backed happy path completes the same operations
  once the one-time configuration above is performed.

The generic revision path is covered by Central's realistic event/API and
publication tests, while `npm run test:automation` verifies Rudiment's event
routing, immutable pin, App identity, project-owned setup, PR checks, and
preview `synchronize` trigger. A controlled live `/ai revise` cycle remains the
final post-merge acceptance check because model execution and GitHub Pages
publication require repository credentials.
