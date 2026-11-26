# AI pull request reviews: checks and isolated previews

This repository runs an AI-assisted development flow: an authorised issue
labelled `ai-task` is implemented by OpenCode through the reusable workflow in
`r0w4n/central-ai-workflow` (pinned by commit SHA in
`.github/workflows/opencode-ai-task.yml`), published as a ready-for-review
pull request, checked automatically by the existing repository workflows, and
previewed in the browser through an isolated GitHub Pages deployment.

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

On `main` (Settings > Branches > Branch protection ruleset / protection):

- Require a pull request before merging (merging stays a manual review
  action; the AI automation never merges).
- Require status checks to pass: `Code quality, unit tests, and build`,
  `Browser correctness and accessibility`, `fast` (performance),
  `CodeQL (javascript-typescript)`, `CodeQL (rust)`, and
  `npm and RustSec advisory audits`.

## Failure modes

- **Preview deployment failed** appears as the sticky PR comment with a link
  to the workflow run logs whenever the build or publish stage fails; the
  build job itself is surfaced through the normal PR check.
- Missing or misconfigured App credentials fail the preview publish job with
  an explicit remediation message instead of a generic Git error.
- A failed PR creation in the AI task workflow deletes the pushed branch so
  no orphan `ai/issue-<n>` branch remains.

Used to validate the AI PR preview pipeline (docs/ai-pr-previews.md).

Second commit to validate preview updates on synchronize.
