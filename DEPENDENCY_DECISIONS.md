# Dependency decisions — Rudiment V3

This record is updated from executable spikes and product tests. Versions are pinned in `package-lock.json` / `Cargo.lock`.

## Engraving: alphaTab 1.9.0 → VexFlow 5.0.0 fallback

- Candidate test: `npm run spike:alphatab`, then serve `spikes/alphatab-dist` and open it in Chrome.
- Input: the exact normalized event shape produced by `rudiment-core` for the bundled four-bar paradiddle, converted to percussion alphaTex with R/L beat text and accents.
- alphaTab result: percussion, beaming, four horizontal bars and annotations render. The pinned API exposes `IExternalMediaSynthOutput.updatePosition` only through its enabled player pipeline.
- Blocking observations for Rudiment: changing per-hit model colors requires `renderScore`, which relayouts the horizontal score; the external cursor requires alphaTab's generated playback/MIDI timeline and player state in addition to Rudiment's authoritative Rust/Tone timeline. Both violate the gate's stable per-hit overlays and strict single-clock/simple-cleanup goals without a significant synchronization layer.
- Decision: use VexFlow 5.0.0 with app-owned SVG feedback overlays, playhead and scrolling. This preserves one canonical event timeline and permits DOM-class updates without score relayout. alphaTab remains a dev-only dependency solely so the recorded spike is reproducible; it is not bundled into production.
- Caveat: Rudiment owns score-follow scrolling and supports the documented PoC grid (quarter/eighth/sixteenth); tuplets and dense polyphony are rejected with warnings.

## Audio: Tone.js 15.1.22

- Selected. Tone.Transport is the sole scheduled musical transport. All click and sample sources receive the callback's Web Audio time. Rust owns rational beats and scoring; VexFlow only consumes the derived position.
- Each monitored drum hit creates its own `AudioBufferSourceNode`/gain voice with end-of-source cleanup; overlapping hits do not reuse or cancel a previous source. Decoded sample layers are peak-normalised independently before the velocity curve is applied because the source recordings vary by almost 28 dB; this keeps the timbral layers without making medium-velocity triggers sound missing. Preview, Start and Connect all unlock audio from a user gesture, while a locked context is surfaced rather than silently dropping monitoring.
- Verified in the production Chrome build: all four CC0 velocity layers decode, Preview unlocks the context, the one-bar count-in and click schedule at Tone callback audio times, and the requestAnimationFrame playhead uses the explicit audio→performance clock bridge. Browser acceptance covers 40 rapid overlapping strikes, pause/resume, stop, restart, tempo changes between runs and a two-loop transition without accumulated callbacks. Subjective loudspeaker/headphone listening was not available to Codex and remains a human audio-quality check.

## USB MIDI: WEBMIDI.js 3.3.1

- Selected. The adapter forwards `message.rawData`, timestamp and port identity. It restores a saved input (or selects the sole input), subscribes to connect/disconnect lifecycle events, and keeps Connect as the explicit permission/recovery path. Automated Chrome simulation verifies prior-authorisation auto-connect, Note On/Off, velocity-zero Note On, CC, denial, disconnect/reconnect and a persisted MIDI Learn override. Physical-device verification remains separate evidence.

## Persistence: idb 8.0.4

- Selected as the one IndexedDB wrapper. Schema versioning lives in the adapter; sessions, settings, profiles and imported bytes share one database.

## Rust/browser boundary

- `serde` + `serde-wasm-bindgen` typed snapshots are selected. Individual MIDI hit events cross as typed JS objects, not JSON strings.

