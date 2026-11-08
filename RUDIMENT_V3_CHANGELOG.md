# Rudiment Codex brief — v3 changelog

- Preserves the full P01–P12 product requirements and declarative MIDI + JSON design.
- Elevates the specialist libraries from recommendations to a **mandatory integration-selection gate**, with measurable acceptance evidence and fallback paths.
- Specifies alphaTab external clock integration proof, per-note overlay test and VexFlow fallback.
- Adds legal requirement to preserve alphaTab's visible score attribution, alongside MPL notices and licence-upgrade review.
- Clarifies one audio clock (Tone/Web Audio), audio unlock, correct draw scheduling, and prohibition on competing playback transports.
- Adds raw MIDI timestamp/byte preservation, uncertain profile verification, and MIDI Learn as authoritative fallback.
- Adds third-party sound provenance, committed lockfile, dependency decision record and stronger end-to-end verification.
- Refreshes Codex kickoff prompt with the v3 document name and explicit implementation order.

This is a *revised implementation spec*, not a claim that Rudiment has been implemented or hardware-tested.

## Implementation addendum — R1–R6

- **R1 automatic MIDI lifecycle:** previously authorised saved/sole inputs now connect on startup, port lifecycle listeners recover hot-plugged devices, disconnect pauses an active run, and MIDI Learn remains persisted and authoritative. First-use permission still requires the visible Connect gesture.
- **R2 score feedback:** every expected note can receive stable Excellent/on-time/early/late/missed SVG state without score relayout. Excellent and on-time thresholds are independently configurable and persisted; matching remains deterministic for simultaneous events.
- **R3 continuous playhead:** the playhead interpolates between note anchors on every animation frame, autoscrolls from the same Tone/Web Audio clock, freezes while paused, and resumes from the stored beat without a scheduling-lead-in jump.
- **R4 monitored MIDI audio:** each strike allocates an independent sample voice and cleanup path. Audio lock is explicit, Connect also performs the required user-gesture unlock, and automated coverage schedules forty overlapping hits without voice loss.
  A physical Alesis capture subsequently exposed a 27.6 dB peak mismatch between bundled velocity layers; decode-time peak normalisation now level-matches the layers before applying the velocity curve, preventing quieter layers from being perceived as dropped triggers.
- **R5 rhythmic MIDI notation:** note-off matching preserves raw gate ticks, while a separate onset/meter inference selects written quarter/eighth/sixteenth duration. The new 80 BPM fixture proves 110-tick gates at 480-tick IOIs render as quarter notes; unsupported ambiguity continues to produce warnings rather than invented notation.
- **R6 GitHub Pages:** the Pages workflow runs locked Rust/browser checks, builds once for browser acceptance and again with the repository subpath, verifies required WASM/JS/audio assets, and deploys through the official Pages actions. A live deployment remains unverified until the repository is pushed and the workflow runs.

Hardware assertions remain deliberately separate: Web MIDI reconnect, byte filtering and rapid audio allocation are browser-simulated; no physical USB drum kit or subjective loudspeaker/headphone test was available for this implementation pass.
