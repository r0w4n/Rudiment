# Rudiment test guide

## Automated checks

Run from the repository root after `npm ci` and loading Cargo into `PATH`:

```bash
npm test
npm run build
npm run test:e2e
```

Rust tests cover SMF parsing, absolute tick conversion, metadata, lyric ambiguity, Note On velocity zero, instrument mapping, onset-derived quarter/eighth/sixteenth/triplet values, mixed subdivisions and rests, short-gate quarter-note imports, deterministic optimal one-to-one matching, exact timing boundaries, continuous piecewise scores, input offsets, score rounding, aggregate timing statistics, simultaneous and rapid hits, misses, duplicates, extras and wrong instruments. Browser tests cover the first-run workspace, bundled and file import, saved MIDI library restoration/deletion, readable non-scrolling results, two-hit result dismissal/start behavior, hit-to-start and statistics dialog behavior and persisted Perfect/Good/Early/Late/Miss totals, removed snare demo controls, score-integrated timing feedback, continuous playhead/pause/resume, rapid independent audio voices, keyboard lifecycle, local persistence, responsive layout (including Pixel 8 Pro portrait and landscape viewports), looping and simulated MIDI permission/device/message/reconnect states.

## Demo checklist (no hardware)

1. Open `http://127.0.0.1:5173` in Chrome. Confirm Single Paradiddle, 60 BPM, Demo input and “Hit your drum or press J to start” are visible.
2. Start practice to unlock browser audio; confirm the count-in and exercise click sound.
3. Start. Confirm four accented count-in clicks at 60 BPM, then the playhead begins at bar 1 and follows the notation.
4. Press `J` for demo snare hits. Confirm signed Perfect/Good/Early/Late feedback, all five timing counts in the live strip, completion summary and Statistics dialog, and audible velocity-sensitive snare playback.
   Confirm Perfect notes turn green, Good notes light green, Early notes orange, Late notes purple and expired misses red. The notehead itself must change colour and its accessible label must state the judgement and signed offset. Adjust the Perfect and Good thresholds in Settings and repeat.
5. Start with a drum hit or `J`; pause/resume with Space, then test Stop and Restart. A natural completion must save a real 0–100 result in a readable, non-scrolling modal without leaving the practice workspace. The first new hit should dismiss the results modal; a second hit should begin the next run.
6. Practise the same tempo/input twice; confirm the second result compares against the previous run. Reload and check that the history count remains.
7. Select “Load bundled .mid”. Confirm R/L sticking and accents render through the same import callback. Drag `single-stroke-roll.mid` onto the score and try the ambiguous fixture to inspect its warning.
8. Open settings: preview all three click timbres, toggle the exercise click and app snare independently, change loops, run eight-tap calibration and Reset it.
9. Import `web/public/exercises/quarter-note-reference.mid`. Confirm eight quarter notes render even though each raw MIDI gate is only 110 ticks. Open the MIDI library, reload the saved file, reload the browser and verify it restores the last selected import; delete it from the library and confirm the prompt.

## Physical Chrome + USB drum-kit checklist

These steps require a real kit and must not be reported as passed from browser simulation:

1. Connect the module by USB and open desktop Chrome. Choose Connect MIDI drums and grant MIDI permission.
2. Verify the real manufacturer/name appears, select the intended port, and expand diagnostics. Strike multiple pads and confirm timestamp, Note On/Off, channel, velocity and raw data are credible. CC and velocity-zero Note On traffic must not sound or score hits.
3. Choose Learn snare, strike the snare, reconnect the module, and verify the learned note/channel persist. Reload with permission already granted and verify the saved/sole input reconnects without pressing Connect. Confirm an unverified device name never silently selects a vendor mapping.
4. Start a run. Confirm snare hits score as MIDI input; kick/hat near a snare target show wrong-instrument feedback and never satisfy it.
5. Play fast single strokes with “Play app snare” enabled and confirm every strike sounds without intermittent gaps; repeat with the module sound disabled to isolate app monitoring. Then turn off “Play app snare” when listening to the module to confirm there is no doubled sound.
6. Disconnect during count-in/running: the run should pause with a reconnect message. Reconnect and confirm the saved port/profile is restored.
7. Run eight-tap calibration against audible clicks, then move the manual offset in both directions and confirm signed timing feedback changes consistently.
8. On a Pixel 8 Pro, verify the MIDI connection control is visible in portrait before connecting. Connect and confirm the first and subsequent snare strikes are audible with “Play app snare” enabled. Rotate to landscape and verify the header, complete four-bar score, feedback and transport remain visible without page scrolling; rotate back and repeat a short run.

## Test status for this checkout

- Performed without hardware: production and repository-subpath builds, Rust suite, full Chromium suite, first-run layout, notation rendering, short-gate quarter-note import, sample decoding/independent voice allocation, count-in, moving playhead/autoscroll, per-note feedback, automatic result saving and reload persistence.
- Browser-simulated only: permission denied/no-device, prior-authorisation auto-connect, hotplug/reconnect, persisted device mapping, Note On/Off, velocity-zero and CC filtering, rapid 40-strike audio scheduling, and Pixel 8 Pro portrait/landscape sizing with zero page overflow. Simulation is implementation evidence, not hardware acceptance or subjective listening evidence.
- Awaiting physical hardware retest: every item in the physical checklist above, including Pixel 8 Pro USB-MIDI audio after permission, rotation behavior, subjective module latency and doubled-audio verification.
- Awaiting hosted deployment: GitHub Pages can only be confirmed after this checkout is pushed and its workflow is allowed to run. Local project-path asset verification is not a claim of a live deployment.
