# Rudiment — Codex kickoff prompt (v3)

**Model:** GPT-5.6 Sol (`gpt-5.6-sol`), high reasoning (or the closest available Sol tier if the selected model is unavailable).

You are the implementation agent. Read **`RUDIMENT_CODEX_POC_SPEC_V3.md` in full**. It is the authoritative specification, superseding earlier revisions. Build a polished, working Chrome-native MIDI drum practice PoC called **Rudiment** in this repository. The user expects an immediately usable, notation-led EarDrum alternative, not a technology demo, documentation exercise or Melodics-style falling-notes game.

**Do the following without stopping at a plan:**

1. Establish the Rust/WebAssembly + Leptos + browser integration project and run it in Chrome. Keep the domain engine independent of the browser for future platform reuse.
2. **Run the library selection gate early**: prove alphaTab can engrave a parsed MIDI paradiddle with R/L sticking and follow an external audio clock, including per-hit markings. If it cannot pass, switch to VexFlow with an explicit decision record. Choose and prove Tone.js for the single audio scheduler, WEBMIDI.js (or direct Web MIDI), Rust `midly` and one IndexedDB wrapper. Pin versions and record licences/attributions.
3. Deliver one complete vertical slice: a beautiful initial screen, bundled 60 BPM paradiddle, audible 1-bar count-in, configurable metronome with proper click sounds, good recorded snare, readable scrolling notation, J-key/on-screen hits, real early/late/miss/extra judgements, a truthful 0–100 result and **Practise again**.
4. Deliver real Chrome USB MIDI connectivity, honest device recognition + MIDI Learn mapping, diagnostics, latency calibration, imported `.mid` exercises with R/L lyric events, session history and like-for-like progress comparison.
5. Implement and run the tests and browser acceptance gates in the spec. Inspect your **actual rendered UI** and revise it; don't claim beautiful UX on the basis of source code. Clearly label all unverified physical-drum-kit checks. Preserve third-party attribution, including alphaTab's visible score attribution if selected.
6. Provide the source code, README, dependency decision record, licence notices, screenshots (if browser tools permit), exact commands to run/build/test, and a P01–P12 completion matrix with evidence. Do not fabricate results or report silent fallbacks as fully implemented requirements.

**Implementation priorities:** Reliable first-use flow > musical timing accuracy > usable notation/sticking > clear feedback and history > optional bells and whistles. Keep one authoritative timeline for audio, notation and scoring. No backend, accounts, native app shell, fake MIDI input status or stubbed success path.

Only declare PoC complete when the no-hardware end-to-end workflow genuinely runs in Chrome and the precise remaining hardware validation steps are documented.
