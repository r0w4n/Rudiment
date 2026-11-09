# Rudiment

Rudiment is a client-side drum practice app for Chrome. It opens directly on a four-bar single paradiddle at 60 BPM with a one-bar count-in, engraved percussion notation, sampled acoustic snare, keyboard demo input, deterministic scoring, and local practice history. USB MIDI, MIDI-file import, MIDI Learn, calibration, looping and score following use the same canonical Rust exercise timeline.

## Run locally

Prerequisites: current stable Rust, the `wasm32-unknown-unknown` target, Trunk 0.22+, Node.js 20+ and npm.

```bash
rustup target add wasm32-unknown-unknown
cargo install trunk --locked
source "$HOME/.cargo/env"
npm ci
npm run dev
```

Open [http://127.0.0.1:5173](http://127.0.0.1:5173) in desktop Chrome. Starting practice unlocks browser audio. No account, server, registration or MIDI device is required; press `J` for demo snare input. Space pauses/resumes and `R` restarts when focus is not in a form field.

The development server writes its disposable hot-reload build to `web/dist/`; release builds continue to use the repository-level `dist/`. Its watcher is restricted to source paths and uses Trunk's post-build cooldown, preventing macOS copy events or generated output from retriggering the build.

## Build and test

```bash
source "$HOME/.cargo/env"
npm run build
npm test
npm run test:e2e
```

The static release is written to `dist/`. `npm run build` and `npm run dev` deterministically regenerate the ignored bundled SMF files. `npm run spike:alphatab` rebuilds the ignored renderer-selection spike.

## GitHub Pages

`.github/workflows/pages.yml` tests the application, builds one relocatable release for both Playwright and GitHub Pages, verifies the deployable app asset graph, and deploys through the official Pages actions after every push to `main` (or a manual workflow dispatch). The workflow uses the hosted runner's bundled Chrome and an integrity-checked prebuilt Trunk binary, avoiding a custom browser image and a source build of the deployment tool. Before the first deployment, set **Settings → Pages → Build and deployment → Source** to **GitHub Actions**; repository settings cannot be changed by the workflow's restricted `GITHUB_TOKEN`. No custom secrets are required. The deployed URL is shown on the workflow's `github-pages` environment after a successful run.

## Architecture

- `crates/core`: web-independent Rust domain model, SMF normalisation with `midly`, instrument mapping, onset-grid and rhythmic-value inference, configurable per-note timing classification, one-to-one matching and Score v1 aggregation.
- `web/src`: Leptos state machine and accessible product UI. Rust/JS snapshots cross with `serde-wasm-bindgen`.
- `web/js/bridge.js`: narrow browser adapter for Tone.js/Web Audio, WEBMIDI.js, VexFlow and IndexedDB (`idb`). Tone.Transport is the sole musical scheduler; scoring and the requestAnimationFrame playhead share its explicitly bridged timeline.
- `web/public`: source-verified CC0 acoustic snare layers, versioned JSON prescriptions/profiles, and build-time generated MIDI fixtures and browser bundle.

## Timing assessment

Scoring v2 uses calibrated Web MIDI timestamps against the musical transport clock. Hits within ±20 ms are Perfect; ±20–40 ms are Good; −40–100 ms are Early; +40–100 ms are Late; and notes outside ±100 ms are missed. The Perfect and Good boundaries are centralised configuration values, while ±100 ms is the default matching limit.

Each expected note contributes continuously: 100 points through the Perfect boundary, a linear 100→80 points across the Good band, then a linear 80→1 points through the Early/Late band. Misses contribute zero and extras contribute no points or penalty. The session score is the mean across expected notes, rounded to one decimal place; any non-Perfect result keeps the displayed score below 100%. Results also retain signed error, mean absolute error and standard deviation for timing-bias and consistency feedback.

All imports, settings, mappings and sessions remain in IndexedDB on the current device. Imported MIDI files are available in a local library, and session summaries can be reviewed in the Statistics dialog. There is no telemetry or upload path.

## Supported scope

Desktop Chrome is the hardware target because Web MIDI support varies by browser. Demo mode works in modern Chromium browsers without permission. Once MIDI permission has been granted, Rudiment restores the saved input automatically, selects a sole available input, and listens for reconnects; the explicit Connect control remains the first-use permission gesture and recovery path. The MIDI importer supports PPQ SMF 0/1, tempo/time-signature metadata, ordinary percussion tracks, onset-derived quarter/eighth/sixteenth and triplet values, R/L lyric events and short practice files. Raw note-gate duration is preserved as metadata but never mistaken for written rhythm. MIDI tempo maps drive playback and scoring, and imported files are saved in a browser-local IndexedDB library with the most recently selected import restored on startup. SMPTE timebases, dense polyphony and arbitrary notation remain outside this PoC.

Only the standards-based General MIDI fallback is bundled. No Alesis mapping is claimed without a physical verification; MIDI Learn is authoritative for unknown kits. See [TESTING.md](TESTING.md), [DEPENDENCY_DECISIONS.md](DEPENDENCY_DECISIONS.md) and [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

The original MVP scope and acceptance baseline are retained in [RUDIMENT_CODEX_POC_SPEC_V3.md](RUDIMENT_CODEX_POC_SPEC_V3.md). Where later implementation requirements differ—particularly scoring v2—the current behaviour documented in this README and the automated tests takes precedence.
