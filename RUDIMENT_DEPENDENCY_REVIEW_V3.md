# Rudiment — v3 dependency decisions and Codex adoption checklist

Date: 8 November 2026. This companion document augments, but does not replace, `RUDIMENT_CODEX_POC_SPEC_V3.md`.

## Library candidates, in priority order

| Responsibility | Preferred | Fallback | Decision gate |
|---|---|---|---|
| Percussion notation and cursor/scroll | `@coderline/alphatab` (MPL-2.0 at time of review) | VexFlow (MIT) | Imported MIDI + R/L + accents + follow + per-hit overlay + loops pass in actual Chrome build |
| Audio/metronome + samples | Tone.js (MIT) | Small direct Web Audio scheduler | Count-in, subdivision, tempo, sound previews and snare all driven by one AudioContext clock |
| USB MIDI | WEBMIDI.js (Apache-2.0) | Browser Web MIDI API | Preserve raw events and timestamps; device permission/disconnect works |
| Standard MIDI File parser | Rust midly | No planned substitute | SMF0/1 + Lyric meta event fixtures pass |
| Browser storage | Rust rexie or idb | One JS/TS IndexedDB wrapper | Persist history and mappings, plus migration test |
| Rust ↔ JS boundary | serde-wasm-bindgen | Explicit typed binding | No lossy high-frequency event transport |
| UI | Leptos + CSS/Tailwind | Minimal vanilla CSS | Intuitive, accessible and performance-stable |
| MIDI simulation/E2E | Playwright + JZZ-compatible test utilities | Mock MIDI adapter | No-hardware tests distinguished from physical USB testing |

## Verification details

1. **alphaTab API:** Its model supports constructing a `Score` programmatically, and published `IExternalMediaSynthOutput.updatePosition(...)` supports external playback-position updates. Verify the exact method wiring in the installed version; never run its own synth transport alongside Tone.Transport.
2. **alphaTab attribution:** Current package/legal guidance requires preserving required notices and the notation attribution shown by the renderer. A future licence change is planned; pin and review on upgrades. If this is unacceptable, choose VexFlow *before* integrating the score engine.
3. **Tone.js:** Schedule audio at the callback's provided `time`; don't use `setInterval` timing for clicks. Unlock AudioContext on a genuine user interaction. Draw score feedback from the current authoritative position (e.g. rAF/Tone.Draw).
4. **WEBMIDI.js:** A convenience layer, *not* a library of verified manufacturer drum mappings. Device-profile matching must allow confirmation and MIDI Learn fallback. Preserve raw bytes and high-resolution timestamps.
5. **midly:** `MetaMessage::Lyric` exposes MIDI lyric meta-events but there is no general standard for associating them with drum sticking. Validate R/L conventions against real EarDrum exports or be precise about supported fixtures and ambiguity.
6. **Good snare sound:** source licensed acoustic recordings, document provenance, and let users separately mute module and app snare. If no convincing licensed sample can be obtained, mark P03 incomplete instead of pretending a synthetic click is a snare.
7. **Evidence:** Codex writes `DEPENDENCY_DECISIONS.md` with the actual version, test, result, selection and outstanding risks. This file is a review, not pre-emptive proof that any integration works.

## Reference documentation

- alphaTab: https://www.alphatab.net/ ; https://next.alphatab.net/docs/features/extend/programmatic-score-construction ; https://alphatab.net/docs/reference/types/synth/iexternalmediasynthoutput ; https://www.alphatab.net/docs/attribution
- VexFlow: https://github.com/vexflow/vexflow
- WEBMIDI.js: https://webmidijs.org/docs/
- Tone.js: https://tonejs.github.io/ ; https://github.com/tonejs/tone.js/wiki/Transport
- midly: https://docs.rs/midly/latest/midly/enum.MetaMessage.html
- IndexedDB: https://docs.rs/rexie/latest/rexie/ ; https://docs.rs/idb/latest/idb/
- Browser MIDI mock: https://github.com/jazz-soft/JZZ
