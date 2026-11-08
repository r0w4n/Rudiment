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
- **R2 score feedback:** every expected note can receive stable Perfect/Good/Early/Late/Miss SVG state without score relayout. Perfect and Good thresholds are independently configurable and persisted; scoring v2 matching remains deterministic for simultaneous events.
- **R3 continuous playhead:** the playhead interpolates between note anchors on every animation frame, autoscrolls from the same Tone/Web Audio clock, freezes while paused, and resumes from the stored beat without a scheduling-lead-in jump.
- **R4 monitored MIDI audio:** each strike allocates an independent sample voice and cleanup path. Audio lock is explicit, Connect also performs the required user-gesture unlock, and automated coverage schedules forty overlapping hits without voice loss.
  A physical Alesis capture subsequently exposed a 27.6 dB peak mismatch between bundled velocity layers; decode-time peak normalisation now level-matches the layers before applying the velocity curve, preventing quieter layers from being perceived as dropped triggers.
- **R5 rhythmic MIDI notation:** note-off matching preserves raw gate ticks, while a separate onset/meter inference selects written quarter/eighth/sixteenth duration. The new 80 BPM fixture proves 110-tick gates at 480-tick IOIs render as quarter notes; unsupported ambiguity continues to produce warnings rather than invented notation.
- **R6 GitHub Pages:** the Pages workflow runs locked Rust/browser checks, builds one relocatable artifact for browser acceptance and deployment, verifies required WASM/JS/audio assets, and deploys through the official Pages actions. It uses the hosted runner's Chrome and an integrity-checked prebuilt Trunk binary. The repository's Pages source must be set to **GitHub Actions** once in repository settings; a restricted workflow token cannot modify that setting. A live deployment remains unverified until the repository is pushed and the workflow runs.

Hardware assertions remain deliberately separate: Web MIDI reconnect, byte filtering and rapid audio allocation are browser-simulated; no physical USB drum kit or subjective loudspeaker/headphone test was available for this implementation pass.

## Implementation addendum — Timing assessment and scoring v2

- Replaced the forgiving hit-rate blend with a transparent per-note mean: Perfect ±20 ms = 100, Good through ±40 ms interpolates to 80, Early/Late through ±100 ms interpolates to 1, and misses contribute zero. Extras are recorded but neither earn nor remove points.
- Added maximum-cardinality, minimum-error one-to-one matching per mapped instrument, with MIDI note identity as a deterministic preference. Rapid repeats, simultaneous drums and duplicate events cannot reuse an expected or observed hit.
- A non-Perfect note prevents a displayed 100.0 score. Scores retain one decimal place and empty exercises remain unscored.
- Session results now retain expected/actual timestamps, signed errors, individual note scores, average offset, mean absolute error and population standard deviation. Misses are excluded from timing-bias and consistency calculations.
- Live and saved summaries expose Perfect, Good, Early, Late and Miss counts. Stave noteheads use green, light green, orange, purple and red with an accessible text label; unmatched physical strikes remain separate red timestamp markers.

## Implementation addendum — Multi-line score progression

- Score bars now wrap into responsive stave systems sized to the notation viewport, with no horizontal scrolling of the full exercise.
- The playhead interpolates within each bar and moves to its next bar/system using the same beat position supplied by the playback clock.
- Vertical viewport tracking eases toward the active system on playback frames, keeping the active and following system visible without re-rendering during playback.
- Resize-only reflow preserves the current beat and judgement feedback; fresh sessions return to the opening system.
- Detected-hit timing feedback uses slightly larger, more vivid colored circles behind noteheads; missed notes do not draw red circles around noteheads.
- Unmatched-strike dots now use the nearest rendered notehead's vertical position rather than a fixed stave-origin offset; hit halos are slightly larger and have a soft color-matched glow.
- End-of-run score summaries now appear in a compact, dismissible modal centered over the score panel, leaving the rest of the practice page in place.
- Result metrics now use readable high-contrast text and a compact layout that fits without scrolling; the first drum hit dismisses results and the second starts the next run. A hit can also start practice from idle.
- Removed the local-save kicker from the result modal and enlarged the score wheel for a more spacious summary header.
- Session results and saved practice statistics now count perfect, early, and late hits individually, including per-session counts in the recent-session list.
- Fresh runs explicitly clear successful-hit halos so no hit circles carry over into a run with no drum input.
- Playback no longer dims past notes or adds a glow to the current note, keeping note glyphs visually stable as the playhead passes.
- The smoothly interpolated playhead now uses rendered notehead positions as beat anchors, aligning its position with each written note without snapping at note times.
- Playhead height now follows each percussion staff's actual top and bottom line coordinates, so it crosses the full stave on every system, including single-line rudiments.
- Adjacent measures share continuous stave lines and a single barline; playhead interpolation reaches the next measure's opening position without a boundary snap.
- Measures on each system are formatted as one continuous rhythmic voice, keeping note spacing consistent across barlines.

## Implementation addendum — Drum-score engraving

- Scores use a conventional five-line percussion stave by default, including snare-only rudiments. Exercises can explicitly select a single-line stave when appropriate; mixed drum-kit MIDI uses GM drum voices placed by MIDI note, including cross/diamond noteheads for cymbals and other percussion.
- Simultaneous MIDI hits are engraved as chords, with durations, dotted values, rests, accents, sticking, and beams formatted by VexFlow.
- System spacing now uses a shared measure width; short final systems keep their proportional width. Measure numbers and increased system spacing improve score navigation.
- Bar lengths and playback/count-in measure boundaries use the numerator and denominator, including compound meters such as 6/8. Playback timing remains based on the MIDI event beats.

## Implementation addendum — MIDI tempo maps and practice workflow

- MIDI onset inference now selects rhythmic grids from observed intervals, including triplets and genuine gaps, while preserving raw gate durations as metadata.
- Parsed tempo-map events are carried into Tone.Transport playback, playhead progress and session scoring so tempo changes remain aligned with musical beats.
- Imported MIDI files are deduplicated and stored in a browser-local library, with load, delete and last-selected-file restoration.
- Added a Statistics dialog for session count, personal best, average hit rate and recent results; removed the demo snare preview/pad while retaining keyboard, MIDI and app audio input.
- Added regression coverage for tempo-map scoring, triplet engraving, statistics behavior and the saved MIDI library.
