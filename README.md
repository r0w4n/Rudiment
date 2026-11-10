# Rudiment

Rudiment is a client-side drum practice app for Chrome. It opens directly on a four-bar single paradiddle at 60 BPM with a one-bar count-in, engraved percussion notation, sampled acoustic snare, keyboard demo input, deterministic scoring, and local practice history. Its MIDI library includes 11 historically rooted, modernised rudiment exercises with reference engravings. USB MIDI, MIDI-file import, MIDI Learn, calibration, looping and score following use the same canonical Rust exercise timeline.

## Run locally

Prerequisites: current stable Rust, the `wasm32-unknown-unknown` target, Trunk 0.22+, Node.js 20+ and npm.

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
source "$HOME/.cargo/env"
npm ci
npm run hooks:install
npm run dev
```

Open [http://127.0.0.1:5173](http://127.0.0.1:5173) in desktop Chrome. Starting practice unlocks browser audio. No account, server, registration or MIDI device is required; press `J` for demo snare input. Space pauses/resumes and `R` restarts when focus is not in a form field.

The development server writes its disposable hot-reload build to `web/dist/`; release builds continue to use the repository-level `dist/`. It watches both Rust/Trunk inputs and `web/js/bridge.js`, rebuilding the browser bridge before Trunk reloads the page. The watchers are restricted to source paths and use Trunk's post-build cooldown, preventing generated output from retriggering its own build.

## Build and test

```bash
source "$HOME/.cargo/env"
npm run check
npm run test:e2e
npm run security
```

The static release is written to `dist/`. `npm run build` and `npm run dev` deterministically regenerate the ignored bundled SMF files. `npm run spike:alphatab` rebuilds the ignored renderer-selection spike.

`npm run hooks:install` opts this checkout into the tracked hooks under `.githooks/`. The pre-commit hook performs formatting, JavaScript syntax and Rust compilation checks; the pre-push hook runs Clippy, Rust tests and the production build. The full browser suite remains in CI so routine pushes do not incur an extra minute locally. `npm run check` runs the complete non-browser quality gate on demand.

## GitHub Pages

`.github/workflows/pages.yml` tests the application, builds one relocatable release for both Playwright and GitHub Pages, verifies the deployable app asset graph, and deploys through the official Pages actions after every push to `main` (or a manual workflow dispatch). The workflow uses the hosted runner's bundled Chrome and an integrity-checked prebuilt Trunk binary, avoiding a custom browser image and a source build of the deployment tool. Before the first deployment, set **Settings → Pages → Build and deployment → Source** to **GitHub Actions**; repository settings cannot be changed by the workflow's restricted `GITHUB_TOKEN`. No custom secrets are required. The deployed URL is shown on the workflow's `github-pages` environment after a successful run.

Dependency security is checked separately by `.github/workflows/security.yml` on lockfile changes and every Monday. RustSec audits Cargo dependencies and npm audits the JavaScript lockfile; Dependabot monitors Cargo, npm and GitHub Actions weekly with grouped updates.

## Architecture

- `crates/core`: web-independent Rust domain model, SMF normalisation with `midly`, instrument mapping, onset-grid and rhythmic-value inference, configurable per-note timing classification, one-to-one matching and Score v1 aggregation.
- `web/src`: Leptos state machine and accessible product UI. Rust/JS snapshots cross with `serde-wasm-bindgen`.
- `web/js/bridge.js`: narrow browser adapter for Tone.js/Web Audio, WEBMIDI.js, VexFlow and IndexedDB (`idb`). Tone.Transport is the sole musical scheduler; scoring and the requestAnimationFrame playhead share its explicitly bridged timeline.
- `web/public`: source-verified CC0 acoustic snare layers, versioned JSON prescriptions/profiles, and build-time generated MIDI fixtures and browser bundle.

The built-in MIDI exercises live under `web/public/exercises/rudiments`. Their matching PNG/SVG engravings, manifest and provenance material live under `tests/fixtures/rudiment-reference`; those test-only references are not linked from the app or included in the deployed site. Automated Rust and browser tests decode each supplied PNG, derive its system structure, and compare that with the generated score alongside the manifest's note, sticking, accent, grace-note, tuplet, bar and tempo expectations. On failure, CI retains the reference and generated images together for visual review; no pixel-perfect identity is required.

## Timing assessment

Scoring v2 uses calibrated Web MIDI timestamps against the musical transport clock. Hits within ±15 ms are Perfect; ±15–30 ms are Good; −30–90 ms are Early; +30–90 ms are Late; and notes outside ±90 ms are missed. The Perfect and Good boundaries are user-adjustable settings, while ±90 ms is the matching limit.

Each expected note contributes continuously: 100 points through the Perfect boundary, a linear 100→80 points across the Good band, then a linear 80→1 points through the Early/Late band. Misses contribute zero and extras contribute no points or penalty. The session score is the mean across expected notes, rounded to one decimal place; any non-Perfect result keeps the displayed score below 100%. Results also retain signed error, mean absolute error and standard deviation for timing-bias and consistency feedback.

All imports, settings, mappings and sessions remain in IndexedDB on the current device. Imported MIDI files are available in a local library, and session summaries can be reviewed in the Statistics dialog. There is no telemetry or upload path.

## Supported scope

Desktop Chrome is the hardware target because Web MIDI support varies by browser. Demo mode works in modern Chromium browsers without permission. Once MIDI permission has been granted, Rudiment restores the saved input automatically, selects a sole available input, and listens for reconnects; the explicit Connect control remains the first-use permission gesture and recovery path. The MIDI importer supports PPQ SMF 0/1, tempo/time-signature metadata, ordinary percussion tracks, onset-derived quarter/eighth/sixteenth and triplet values, R/L lyric events and short practice files. Raw note-gate duration is preserved as metadata but never mistaken for written rhythm. MIDI tempo maps drive playback and scoring, and imported files are saved in a browser-local IndexedDB library with the most recently selected import restored on startup. SMPTE timebases, dense polyphony and arbitrary notation remain outside this PoC.

Only the standards-based General MIDI fallback is bundled. No Alesis mapping is claimed without a physical verification; MIDI Learn is authoritative for unknown kits. See [TESTING.md](TESTING.md), [DEPENDENCY_DECISIONS.md](DEPENDENCY_DECISIONS.md) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

The original MVP scope and acceptance baseline are retained in [RUDIMENT_CODEX_POC_SPEC_V3.md](RUDIMENT_CODEX_POC_SPEC_V3.md). Where later implementation requirements differ—particularly scoring v2—the current behaviour documented in this README and the automated tests takes precedence.
