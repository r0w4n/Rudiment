# Codex implementation brief — Rudiment PoC (v3)

**Revision:** 3 — 8 November 2026. Library-first implementation and verifiable integration contracts. **Supersedes v2.**

**Audience:** GPT-5.6 Sol (`gpt-5.6-sol`, high reasoning), acting as an autonomous senior product engineer, Rust/WebAssembly engineer, frontend designer, audio/MIDI specialist, and test engineer.

**Status:** Implementation specification. Build the application; do not merely produce a plan or screenshots.

**Product:** Rudiment. A polished, notation-led, browser-native Chrome application for electronic-drum rudiment practice. This is closer to EarDrum than Melodics. It is not a falling-notes game, lesson subscription service, or DAW.

## 0. Your mission and definition of success

You own the outcome, not just the code. Create the smallest **complete and satisfying** practice experience that a drummer can use on first launch. The target user has an electronic drum kit (initial hardware target: an Alesis Turbo-family USB MIDI kit), values deliberate rudiment practice, and wants to import MIDI exercises, see left/right sticking, practise to a good metronome, hear a convincing snare, and objectively compare successive attempts.

The first experience must feel like a thoughtfully designed real application, not a proof-of-technology page. Prioritise correct timing, great feedback, legible notation, frictionless setup, clear states, graceful failures, and credible audio quality over feature count or novel architecture.

**Non-negotiable outcome:** A user can open the app in desktop Chrome, immediately try a built-in 60 BPM paradiddle exercise without hardware, connect a USB MIDI kit when present, run an exercise with a count-in, see expected notes/sticking and per-hit feedback in sync, finish the run, see a meaningful result, and improve/compare on the next run. A local `.mid` file containing sticking can be imported and practised.

Work independently through implementation, testing, visual inspection, fixes, and documentation. Do not stop after scaffolding or deliver a static mock-up. Ask for input only if an external blocker truly prevents any useful continuation. Choose sensible defaults and document them.

## 1. Scope and constraints

**Target runtime**
- Desktop Google Chrome (current stable) on macOS and Windows; localhost for development and HTTPS in deployment.
- Browser-native web app. **No Android native app, Electron/Tauri wrapper, backend, accounts, login, or database service.** No assumption of Web MIDI support on iPadOS.
- The application must run entirely client-side; imported MIDI files and practice history stay on the device.
- Responsive desktop/laptop layout, with a usable narrow-width fallback. Desktop Chrome + physical USB MIDI is the hardware acceptance target.

**Core technology**
- **Rust compiled to WebAssembly** for MIDI-file parsing, normalisation, exercise timing model, deterministic hit matching/scoring, instrument mappings, and session summaries.
- **Leptos** with HTML/CSS for the reactive UI, unless there is a reproducible compatibility obstacle; explain any deviation. Use a small amount of JavaScript/TypeScript only for browser-specific integration where it is demonstrably more practical.
- **Web MIDI API** for connected MIDI inputs. Require secure context/permission; support hot-plug and reconnect. Do not request SysEx unless strictly necessary (not expected for PoC).
- **Web Audio API** for metronome and sample playback. Schedule sounds against the audio clock using lookahead; do not rely on `setInterval` callbacks for the beat itself.
- Use a mature score renderer: **evaluate alphaTab first**, with **VexFlow** as fallback. See Dependency Selection Gate below. Never build an engraving engine from scratch; pin versions and verify API contracts.
- **Library-first is mandatory, not merely a suggestion.** Use a proven dependency for engraving, MIDI browser integration, music scheduling, SMF decoding, and persistence, unless a documented running integration test demonstrates it would degrade correctness or user experience. Do not replace mature functionality with custom code to pursue theoretical Wasm performance.
- `midly` (Rust) is the preferred Standard MIDI File parser (including raw lyric meta-events); `serde` / `serde-wasm-bindgen` for data and JS boundaries. Use a single IndexedDB wrapper (`rexie` or `idb`) for persistence. Verify current compatible releases.
- Static deployability; simple reproducible local development command. A PWA manifest/offline cache is nice-to-have, but must not delay core quality.

**Keep clean seams for future iOS:** shared domain logic must not depend on DOM, Web MIDI, Web Audio, or Leptos types. Use adapters at system boundaries. Future iOS is not part of this build.

## 1A. Dependency Selection Gate — required before committing to the renderer/audio architecture

**Purpose:** Reuse mature libraries to accelerate the first usable version, without sacrificing the exact timing/scoring contracts. Time-box this to short executable integration spikes and then build the product. Do not spend the project benchmarking frameworks indefinitely.

### Notation and scrolling (highest impact)

**First candidate: `@coderline/alphatab` (MPL-2.0)** — percussion notation, annotations/lyrics, beat bounds, playback cursor, horizontal scrolling, and programmatic `Score` construction. It **does not directly import `.mid` as notation**. Use `midly` to parse MIDI into Rudiment's exact, rational-time `ExerciseEvent` model, then construct an alphaTab score using its programmatic model API (or alphaTex only if demonstrably simpler). Verify that this conversion accurately handles eighth/sixteenth notes, rests, accents, and per-note R/L labels.

In a small running Chrome spike, prove **all five**:
1. Programmatically render the actual bundled `.mid` paradiddle (not handwritten UI notes) with snare staff and correctly aligned R/L + accents.
2. Render 4+ bars and continuously follow the playhead without horizontal jumps or distracting rerenders.
3. Drive the score from Rudiment's **single authoritative exercise/audio clock** using alphaTab's external-media/cursor APIs rather than using an unsynchronised second transport.
4. Overlay or recolour an individual note for on-time, early, late, and missed states without destroying performance or notation layout.
5. Load, redraw, resize, restart and loop without orphan listeners, audible doubles, cursor drift or stale state.

If any hard requirement fails and cannot be remedied simply, adopt **VexFlow** (MIT) with an app-owned playhead and scrolling container. With VexFlow, Rudiment must implement cursor-follow/scrolling itself, but remains in control of per-note visual feedback. Do not combine two production engraving engines.

### Audio and metronome

**First candidate: `tone` (MIT)** for the Web Audio scheduler/Transport and sampled snare/metronome playback. Tone.js is a scheduling engine, **not a source of premium drum samples**. Supply actual locally bundled, provenance-checked sound assets and verify instrument quality by listening. Every metronome event must be scheduled at Tone's provided AudioContext time, not the callback's wall-clock time. Keep separate volumes and the ability to mute reference snare when the drum module produces its own audio.

**One clock rule:** choose **one** audio transport / master timeline. Do not independently run alphaTab's synth transport and Tone.Transport as unsynchronised parallel timekeepers. Recommended path: Tone schedules audio; Rust maintains the rational-time exercise timeline and performance/scoring model; the notation adapter follows the corresponding authoritative position. Correctly bridge Web MIDI's DOMHighResTimeStamp/performance clock to the audio clock and apply the user's measured offset. If alphaTab's external-clock integration proves prohibitively complex, prefer VexFlow.

### USB MIDI and instrument identification

**First candidate: `webmidi` / WEBMIDI.js v3 (Apache-2.0)** for device enumeration, note-on/control-change callbacks, disconnect handling and timestamps in a small TypeScript adapter. Use the raw Web MIDI API instead if this abstraction adds complexity or hides raw events. Regardless of library, preserve raw status bytes, channel, note/CC, velocity, original event timestamp and port identity for diagnostics and scoring. **WEBMIDI.js is not a manufacturer-specific electronic drum profile database.** Preserve the app's JSON mapping profiles and MIDI Learn fallback; don't promise automatic identification of every kit from port names.

### Parsing, storage, binding and UI

- `midly` (Rust): parse SMF tracks, delta ticks and `MetaMessage::Lyric` without lossy conversions. Preserve raw lyric-event time/track and explicitly determine R/L association; do not assume EarDrum has a publicly standardised mapping convention. Bundle fixture MIDI files and test against them.
- `serde` + `serde-wasm-bindgen`: transfer structured, typed snapshots between Rust/Wasm and the small JS adapters; avoid repeatedly serialising raw hit events into JSON strings.
- `rexie` **or** `idb` (Rust/Wasm): one IndexedDB abstraction for import originals, settings, device maps and immutable run summaries; test persistence and migrations.
- `Leptos` remains the primary UI. Optionally use `daisyUI` with Tailwind for consistent, accessible styling primitives (CSS-only; not a substitute for bespoke practice-screen interaction). Avoid introducing React purely to adopt a component kit.
- Dev/testing only: consider `web-midi-test` from the JZZ ecosystem for realistic simulated Web MIDI events and disconnection cases. Use Playwright for Chrome end-to-end UX. Distinguish mocked browser tests from physical-kit verification.

### Asset, licensing and performance rules

- Record exact package versions, licence types, attribution obligations and bundled audio provenance in `THIRD_PARTY_NOTICES.md`. alphaTab uses MPL-2.0; VexFlow MIT; Tone.js MIT; WEBMIDI.js v3 Apache-2.0. Do not embed arbitrary sound recordings from the internet.
- Lazy-load notation/audio components if necessary, keep the startup experience quick, and benchmark actual interaction/latency before optimising Rust/Wasm microcode. Remove unused heavy dependencies and avoid competing audio engines.
- This evaluation is not an excuse to deliver a tech demo: the same functional P01–P12 and first-run acceptance tests still apply.

**Evidence:** In `DEPENDENCY_DECISIONS.md`, record the selected renderer and audio stack, what was verified in a real browser, observed limitations, and why rejected alternatives would slow or risk delivery. Do not mark an integration as verified until its spike actually runs.

### Concrete library-selection contract (v3)

| Concern | Preferred package | Plan B | Acceptance evidence |
|---|---|---|---|
| Music engraving + scrolling | `@coderline/alphatab` | `vexflow` | Render imported fixture, correct R/L, accents, beat-follow, stable note feedback |
| Musical scheduling + samples | `tone` | Small Web Audio scheduler only if proved necessary | Accurate count-in/click/sample against one AudioContext timeline |
| Browser USB MIDI | `webmidi` (WEBMIDI.js) | Native Web MIDI API | Raw timestamp, note/CC, hotplug, permissions, diagnostics preserved |
| SMF import / lyric extraction | Rust `midly` | None without a documented blocker | Typed parser fixtures for SMF0/1, lyric metadata, multi-track, malformed data |
| Browser/UI transport | `serde`, `serde-wasm-bindgen` | Explicit bounded native bindings | Typed events; no JSON stringifying on every drum hit |
| Local storage | Rust `rexie` **or** `idb` | A single TypeScript IndexedDB wrapper | Session persistence, profile persistence, migration test |
| UI styling | Leptos + CSS/Tailwind | Minimal CSS without Tailwind | Responsive, accessible and polished practice view |
| E2E & MIDI simulation | Playwright + compatible virtual MIDI test tools | Mock adapter + Playwright | Actual browser tests, separately from hardware evidence |

**Selection decision and evidence protocol:**
- Create `DEPENDENCY_DECISIONS.md` **before** the main implementation and update it after each executable spike. Include candidate/version, tested browser, observed behaviour, decision, caveat, and a link to the repository test or demonstration.
- Commit a dependency lockfile. Do not silently switch to a different notation/audio library later. If falling back from alphaTab to VexFlow, record the exact failed capability and prove the fallback before proceeding.
- Treat newer documentation as guidance, not proof that a method exists on the *installed pinned version*. Verify imports, symbols and semantics against the installed package.
- **Specific alphaTab integration point:** its `IExternalMediaSynthOutput.updatePosition(...)` interface exists in published documentation, but the exact wiring into Rudiment's cursor must be demonstrated with the pinned version. Do not assume the renderer will automatically follow Tone.Transport without implementing the adapter and testing it.
- **alphaTab legal/attribution requirement:** MPL-2.0 license notices, source availability information where required, and the library's own displayed notation attribution must be preserved. Never crop, conceal, remove or overlay an embedded alphaTab mark. Its maintainers have announced a possible future licence change with stronger UI-attribution terms: pin and audit upgrades, and if obligations are unacceptable choose VexFlow before locking in.
- **Acoustic samples:** only bundle source-verified, redistributable assets. Include provenance and notices in `THIRD_PARTY_NOTICES.md`; prefer several subtle velocity layers. License compliance is an acceptance criterion, not a cleanup task.
- **Strict single-clock architecture:** Tone (if selected) schedules against the Web Audio clock. AlphaTab's own synthesizer/transport is disabled for exercise playback; the notation renderer only consumes Rudiment's authoritative position. Avoid separate ticking clocks for UI, score, scoring and audio. Count-in, resume and loops must reconcile to one timeline.
- **WebMIDI.js boundary:** the library's friendly note event abstractions cannot discard event timestamps or original bytes. Preserve Web MIDI `MIDIMessageEvent` timing/raw payload where possible, independent of human-friendly names. If the library obscures them, use the native Web MIDI API instead.
- **First-use acceptance is non-negotiable:** a technically successful spike without a fully usable first practice session is not PoC completion. Do not deliver only an engineering demo of the selected libraries.

## 2. Product principles

1. **Practice-first:** score dominates; no cartoon game, no falling tiles, no ads, no clutter.
2. **First meaningful action in seconds:** preload a practice-ready exercise and expose two clear entry points: **Connect MIDI drums** and **Try without drums**.
3. **User's hands are occupied:** large primary controls, readable at a distance, minimal interaction during a run; keyboard shortcuts on desktop.
4. **Honest feedback:** scores must be deterministically computed from actual events; never fabricate a successful MIDI connection or session data. Demo-mode input is visibly identified as simulated/keyboard input.
5. **No dead ends:** unsupported MIDI, denied permission, missing device, muted audio, malformed file, and disconnected kit have actionable recovery UX.
6. **Correctness before micro-optimisation:** Rust/Wasm is chosen for portable, testable musical logic; audio/notation quality outweighs saving a few CPU cycles.
7. **No arbitrary code in exercise data:** exercises and practice rules are declarative and schema-versioned.

## 3. First-run experience — implement as the core vertical slice

### Landing / practice workspace
Show a tasteful, polished, modern studio interface (suggested palette: dark neutral background, warm-white notation, restrained teal/amber accents, high contrast; avoid generic SaaS dashboard aesthetics). Main focus is the score. The user sees:
- Name and description of a preloaded **Single Paradiddle** exercise, default **60 BPM**, 4/4, one-bar count-in, initially 4 bars or repeatable short pattern.
- **Large readable percussion staff** with R L R R L R L L sticking, correct note spacing, accents, bar lines, current beat, and upcoming notes.
- Obvious primary **Start practice** button; **Connect MIDI drums** secondary action; **Try without drums** clearly available.
- Simple tempo stepper/input (30–240 BPM; default 60), sound preview/control, metronome toggle, loop control, stop/restart.
- MIDI connection pill with accurate states (not connected / requesting permission / connected / error). No fake “Connected” status.
- A compact feedback strip that shows Hit / Early / Late / Missed / Extra; plus a small current score or hit ratio. No flashing overload.
- Separate metronome and app-snare volume/mute controls, immediately discoverable.

### Demo mode (mandatory, no USB needed)
- A true end-to-end playable mode using keyboard key **J** and an on-screen snare pad for hits. Space toggles play/pause (except when typing in fields); R restarts. Label the mode **Demo input**, and state that keyboard/pointing-device input has different latency characteristics from USB MIDI.
- Demo hits travel through the **same normalised event, matching, scoring, and history pipeline** as MIDI, with source recorded as `demo`. Do not prepopulate score results or autoplay fake hits.
- Hearing an excellent snare on the first pad tap is a product-quality requirement.
- A complete practice run leads to a real, readable results screen and a **Practise again** button.

### With USB kit
- Prominent **Connect MIDI drums** action invokes browser permission when needed.
- Show discovered input ports by manufacturer/name. Auto-select saved device when reconnected; support explicit device selection.
- On first unknown kit, a **Quick MIDI Learn** wizard: “Hit your snare” → capture MIDI note and channel; optional kick/hat and other pads. Preview incoming note/velocity. Save profile persistently. **Never assume a reported device name guarantees a mapping.**
- Include a GM-compatible fallback profile and an editable Alesis Turbo-family starter profile **only when mapping values are verified**. Otherwise use MIDI Learn as the authoritative setup. The input port name alone is not always reliable or unique.
- A diagnostic expander displays timestamp, note-on/note-off, velocity, CC and channel. Support note-on velocity 0 as note-off; filter note-offs from hit scoring. CC traffic must not create phantom snare hits.
- A *real* device profile is selected only when its name/manufacturer match is plausible **and** the profile can be confirmed by one or more pad hits; otherwise prompt for quick MIDI Learn. Treat unverified maps as suggestions, not supported hardware.
- If the device disappears, pause safely and display a reconnect action; no corrupted session.

## 4. Twelve mandatory PoC capabilities and acceptance intent

| ID | Capability | Working behaviour required |
|---|---|---|
| P01 | USB MIDI | Browser permission, enumeration, select input, receive events, reconnect/error UX, realistic device-state labels. |
| P02 | Configurable metronome | BPM, meter, quarter/eighth/sixteenth subdivisions, downbeat accent, one/two-bar or off count-in, at least **3 musically appropriate click timbres** (e.g., woodblock, crisp click, cowbell), independent volume/mute and audible sound preview. |
| P03 | Good snare sound | Acoustic snare sample(s), velocity-sensitive playback, crisp low-latency response; separate mute/volume from metronome; avoid doubling when hearing module audio. Use legally redistributable samples with attribution in repository. |
| P04 | Import `.mid`/`.midi` | File picker + drag/drop; parse SMF 0/1, PPQ timing, track names, tempo, meter, percussion notes; meaningful error/warning states; track choice when ambiguous; preserve original file locally. |
| P05 | Left/right sticking from MIDI | Read lyric meta-events, interpret supported R/L values in positional alignment with notes, display above/below staff, gracefully handle missing/ambiguous lyrics; preserve raw evidence. Do not claim the association convention is a formal MIDI standard. |
| P06 | Recognise MIDI instrument | Read reported name/manufacturer, offer matching/saved profile, MIDI Learn and persistent overrides. Device mapping independent from exercise notation. |
| P07 | Show notation | Proper percussion stave, rests, bar lines, time signature, note values and beams for supported rhythms, accents when derivable, R/L, playhead and clear completed/upcoming notes. |
| P08 | Per-hit judgement | On-time, early, late, missed, extra and wrong instrument with signed milliseconds; differentiate matched hits from missed notes; readable visual cues not dependent on colour alone. |
| P09 | Exercise lifecycle | Idle → count-in → running → paused → finishing → completed; start, pause, stop, restart; automatic completion after final expected hit window; looping with clear loop count. |
| P10 | Deterministic scoring | 0–100 score derived from expected vs observed notes and timing, with formula and version; explanatory result metrics and predictable outcomes. |
| P11 | Progress comparison | Local history of real sessions; last vs current and personal best **for same exercise/version, tempo, input mode, and scoring version**; simple timeline or prior-run comparison. |
| P12 | Auto-scrolling | Smooth position-following score with current beat visible and lookahead, no jitter or skipped bars; user can choose continuous horizontal or simpler page/bar follow mode. |

**Additional necessary foundations (part of PoC, not optional scope creep):** per-device latency-offset setting and simple calibration flow; useful built-in exercises; repeat/loop controls; MIDI diagnostics; audio unlock/permission UX; persist settings/history; an accessible UI.

## 5. Musical and MIDI data contracts

### Canonical representation
MIDI is **what to play**, JSON is **how to practise**. Keep both declarative. No per-exercise code branches.

Suggested internal types (names flexible):
- `MidiSource`: original bytes, filename, parse warnings, original tempo/meter/ticks-per-beat and source hash.
- `ExpectedHit`: stable ID, musical position as integer ticks or rational beats, canonical instrument (snare/kick/closed-hat/open-hat/etc.), velocity/accent, optional intended hand `L|R|unknown`, source track/channel/note.
- `Exercise`: ID, title, source/content hash, schema version, normalized events, supported time signature and original tempo.
- `PracticePrescription` (JSON): schema version, tempo override, count-in bars, repetitions/loop mode, metronome choices, scoring tolerance. Default values are versioned.
- `DeviceProfile` (JSON): device matcher, individual note/channel→canonical instrument bindings, CC mappings when useful, per-device calibration offset, user overrides.
- `ObservedHit`: `performance.now()`-domain timestamp (or explicitly converted monotonic time), canonical instrument, raw MIDI information, velocity and source (MIDI/demo).
- `Session`: unique ID, start/end, exercise content hash, practice prescription, device/input-mode, tempo, calibration applied, scoring algorithm version, detailed event results and aggregates.

Use a single canonical event/timeline model shared by score rendering, audio scheduling, matching and history. Do **not** separately calculate beat positions in UI, audio and scorer.

### MIDI import handling
- Parse multi-track delta times into absolute ticks; respect time signature and tempo map.
- The PoC prioritises short **percussion/rudiment** MIDI exercises: quarter/eighth/sixteenth rhythms, rests, accents, basic drum voices, and standard PPQ SMFs. Handle imported file tempo metadata but allow a practice-tempo override. If unsupported polyrhythms, unusual timebases, dense overlaps, tuplet engraving, or tempo changes cannot be faithfully displayed, **detect and explain** rather than silently mis-rendering.
- Detect conventional percussion channel 10 (wire channel value 9), **but don't assume all files use it**. Detect tracks with drum-name clues or let the user choose the source track.
- Map standard percussion notes into canonical instruments. At minimum: snare, kick, closed/open hat, pedal hat, ride, crash. Snare-based exercises must be fully playable; if a file contains unsupported note types, warn and let the user choose a supported track, rather than silently dropping notes and reporting a falsely good score.
- Treat drum Note On events as attacks; MIDI note-off duration isn't reliably the written note length. Engraving requires rational rhythmic inference/quantisation. Constrain it to documented supported grid values; display a warning when quantisation changes an event by more than a small threshold, and preserve original timing for audit.
- Associate R/L lyric events by absolute tick; if same-tick multiple notes make assignment ambiguous, mark sticking unknown and explain. Also support single-note aligned lyrics encountered in generated test fixtures. **EarDrum-compatible lyric behaviour requires fixture verification; do not invent claims of perfect compatibility.** Missing sticking displays a neutral marker, not guessed hands.
- Ship at least two built-in, locally bundled MIDI fixtures generated/validated within the project: (1) single-stroke roll with R/L lyrics; (2) single paradiddle RLRRLRLL with accents. Both import through the same path as external files. Provide a fixture with missing/ambiguous lyrics for testing.

### Sound assets
- Source a legitimately redistributable, recorded acoustic snare (prefer several velocity layers/round-robin samples) and short metronome samples. Repository includes attribution, source URLs and licence evidence. **Do not embed copyrighted commercial kit samples without rights.**
- Load and decode samples in advance; on first interaction, resume/unlock AudioContext and show the audio-loading/permission state. Sound preview must work before the user starts the timed exercise; do not start a run silently while audio is blocked.
- If usable licensed snare assets cannot be obtained, deliver an explicitly labelled temporary synthesised fallback **and flag P03 as not fully met**, rather than pretending it sounds professional.
- Preload/decode short sounds before practice; sound preview and metronome should have independently adjustable loudness; avoid clipping or excessive volume defaults.

## 6. Timing, calibration, scoring — correctness contract

**Clock discipline**
- Web MIDI event timestamps are DOM high-resolution timestamps in the browser time domain; Web Audio's `AudioContext.currentTime` uses its own audio-clock domain. Build and test an explicit bridge, preferably via `getOutputTimestamp()` when supported and a documented fallback. Never directly subtract values from different clocks.
- Schedule the actual click and sample playback **ahead on the audio timeline**, e.g. a short periodic scheduler that books the next ~100 ms of notes, rather than calling `play()` at the moment a beat is due. Update the moving playhead using animation frames derived from the authoritative session timeline; visual frame drops must not affect hit scoring.
- Tone.Transport callbacks receive a scheduled audio timestamp: schedule the AudioNode/sample at that timestamp, not when the JavaScript callback executes. UI/notation uses a clock-to-frame adapter (for example, Tone.Draw or a requestAnimationFrame loop reading the authoritative audio position), **not** the pre-scheduled audio callback time as if it were already audible.
- Apply an adjustable calibration offset in milliseconds to input-versus-sounding-beat comparison. Offer a straightforward tap-with-click calibration UI or guided manual adjustment, preserve raw timestamps and calculated corrections, and make **Reset calibration** obvious. Avoid claims that calibration fully removes unknown external audio hardware latency.
- Handle audio context suspended/unlocked state; start audio only after user interaction; pause safely when tab is backgrounded or audio is suspended.

**State machine**
`idle → count_in → running ↔ paused → finishing → complete` plus `cancelled` and `error` where appropriate. The count-in is not scored. Final session completion waits for a final matching window. Restart resets score, audio queues, playhead and loop counters. Do not leak timers/listeners across repeats.

**Hit matching**
- Normalise all raw inputs using the selected profile before evaluation. Match expected hits to observed hits of the **same canonical instrument** with one-to-one pairing, deterministic nearest-time matching within a window; once paired, hits cannot be reused.
- Reference matching window: `W = min(120 ms, 0.4 × nearest distinct expected inter-onset interval)`, with sensible guard for very slow notes and cases with isolated/simultaneous notes. Document actual implementation and tests; dense grace-note cases may be explicitly unsupported at PoC stage.
- Default **on-time:** absolute calibrated error ≤ 35 ms; otherwise matched early or late based on sign; unmatched expected becomes **missed**; unmatched observed becomes **extra**. Don't mark misses until their window has expired. Adjustable timing tolerance may be under Advanced settings, but keep a stable default for comparisons.
- Surface wrong-instrument events distinctly if another mapped instrument is struck near an expected hit. A note on an unrelated instrument must not satisfy a snare target.
- Do not infer the actual left/right striking hand from ordinary MIDI: the app shows **intended** sticking and can score drum/timing, not physically verify hands.

**Score v1 (implement exactly, or state and justify a clearly documented deterministic alternative)**
- Let `N` = number of expected scored hits in the run, `M` = matched correct-instrument hits, `E` = unmatched observed scored hits, `e_i` = absolute calibrated timing error of each matched hit, and `W_i` = its matching-window width.
- `hit_rate = M/N`.
- `timing_quality = (Σ over matched hits clamp(1 − e_i / W_i, 0, 1)) / N` (misses contribute zero).
- `extra_penalty = 0.10 × min(1, E/N)`.
- `score = round(100 × clamp(0.70 × hit_rate + 0.30 × timing_quality − extra_penalty, 0, 1))`.
- Return “not scorable” for `N = 0`. Show M/N, misses, extras, mean absolute timing error, signed mean error (rushing/dragging), and score. Persist score algorithm version `v1` and all underlying hit observations. Treat score as app-specific feedback, not an established drumming-grade standard.
- Sessions with different tempo, exercise hash, input source (demo vs hardware), tolerances, or scorer version must be visibly distinguished and not automatically presented as apples-to-apples improvement.

## 7. UX and visual quality bar

Deliver **one beautiful, well-considered practice screen**, not ten unfinished screens.

**Desktop layout:** central expansive score area (at least two bars lookahead when possible), small top bar (exercise, tempo, input status), restrained bottom transport, contextual metronome/audio controls, lightweight feedback beneath the score. Provide a compact settings drawer for device setup, mappings, calibration and sound. A separate, lightweight results view shows score, matched/missed/extra counts, early/late bias, comparison against previous equivalent run, and **Practise again**.

- Provide a brief welcome hint that disappears after first use, not an obligatory multi-step wizard.
- Format sticking clearly (R/L), avoid cramped notation at default zoom. Use full-window desktop layout without unnecessary scrolling.
- Current playhead obvious; preserve readable future notes; smoothly follow score without jerking at every note. User-selectable continuous horizontal / page-follow modes.
- Use accessible type contrast, semantic buttons, keyboard focus/shortcuts, and visual cues beyond red/green alone. Respect `prefers-reduced-motion`.
- Make no-device/denied-permission states positive: “No MIDI drums detected. Try the practice demo or connect your USB kit.”
- Show user-centred text (“Hit the snare to assign it”) rather than implementation jargon (“CC/0x99”); reserve technical details for MIDI diagnostics.
- Polished micro-interactions: hover/focus states, progress through count-in, subtle hit feedback, clear completed state. No giant modal for every action, no gratuitous confetti, no fake progress counters.
- **Distinctive quality requirement:** demo exercise is visually meaningful and audible on first use; don't ship Lorem Ipsum, skeleton-only UI, placeholders, or decorative dead buttons.

## 8. Persistence, robustness, and privacy

- Store imported MIDI originals, parsed metadata, device profiles/user mappings, metronome settings and completed sessions in IndexedDB. Use schema versioning/migration.
- Do not require an account, external telemetry, remote calls, or uploads. No third-party analytics in PoC.
- Autosave relevant settings. Manual **Reset settings/profile**; **Clear practice history** behind confirmation. Keep a simple local JSON export of history if trivial, otherwise defer.
- Validate untrusted files and profiles: file size/event count limits, malformed message handling, path-independent import, safe text display (no HTML injection), bounded resources. Never execute data in exercises.
- Detect unsupported browser/API with an explicit help state; do not assume USB MIDI is available in Safari/iPad Chrome.
- Audio/MIDI resources and event listeners must be cleaned up on disconnect, rerender or route change.

## 9. Repository deliverables

1. Working source tree with a README describing prerequisites, development commands, build commands, architecture, supported browsers and known limitations.
2. `/crates/core` (or similar) pure Rust domain code: SMF normalisation, instrument model, exercise timeline, hit matcher, scorer, session aggregation and serialisable model. **No web/UI dependencies in core**.
3. `/web` (or similar): Leptos-based application, browser adapters, notation adapter, Web MIDI/Web Audio implementations, local persistence, audio assets and styling.
4. Example versioned JSON device profiles and practice prescriptions; two bundled valid MIDI exercises with R/L lyrics.
5. Third-party sound-asset licences and attributions.
6. Automated test suite: Rust unit/property tests for parsing/matching/scoring and web UI smoke/e2e tests where feasible; no fabricated pass reports.
7. Brief `TESTING.md` with a **real-hardware checklist** for a Chrome + USB kit and a **demo-without-hardware** checklist. Explicitly separate tests actually run from those requiring the user's hardware.
8. Screen captures of the implemented first-run practice screen and results screen if the execution environment supports browser inspection. Review the actual screenshots and improve them; don't claim UI quality without viewing output.
9. `DEPENDENCY_DECISIONS.md`, `THIRD_PARTY_NOTICES.md`, and a committed package lockfile showing exactly which audio, notation, MIDI and persistence packages were selected and why. Document any deviation from the library-first shortlist.

## 10. Acceptance tests — completion gate

### A. No-hardware happy path (must be executable by Codex)
1. Open local app in Chrome. Without permission, registration or setup, a 60 BPM paradiddle and **Try without drums** are visible and styled professionally.
2. Tap Preview Snare: convincingly acoustic sound is audible (subject to local playback verification), not merely a harsh synthesized beep; if asset unavailable, report incomplete.
3. Select/start exercise. One full bar of count-in sounds at configured metronome timbre; notation playhead starts at correct bar after count-in and scrolls reliably.
4. Use keyboard `J` or on-screen snare to input hits. Show live judgement. Record actual timestamp-derived results, no simulated successes.
5. Pause, resume, stop and restart leave correct state; an automatically completed exercise produces result with score and timings.
6. Run the same exercise twice. Second result shows previous comparable run and explains improvement/decline. Reload page: history persists.
7. Import bundled `.mid` file through **same user-facing import path**; display percussion notation and correctly aligned R/L markers; practise it.

### B. Hardware path (manual if hardware is absent)
1. Connect actual USB e-drum to computer; Chrome prompts for MIDI, lists real input, selects it; live diagnostics display note/channel/velocity/timestamp.
2. Snare-hit MIDI Learn captures correct note; subsequent hits trigger the app's snare and in-run scoring. Note-off and CC do not trigger phantom hits.
3. Disconnect/reconnect is signalled; saved profile is restored. External module audio can be used without doubled app-snare playback via an obvious mute control.
4. Test latency calibration with audible click; adjust offset and verify timing feedback direction changes consistently.
5. Exercise run with real drum kit produces meaningful timing/miss/extra feedback and stored score labelled `MIDI`, not `demo`.

### C. Automated correctness tests
- SMF0/SMF1, multiple tracks, delta-to-absolute conversion, tempo/time signature, channel-10 detection, note-on velocity zero, truncated/corrupt file, lyric alignment and ambiguous lyric cases.
- RLRRLRLL fixture has expected hit count and R/L sequence at correct positions.
- Scorer: perfect events score 100; consistent early vs late classified by sign; missing and extra notes reduce score; wrong-instrument hit never matches; simultaneous/multiple close hits cannot be double-matched; final hit is not prematurely marked missed; pause/count-in does not contribute hits.
- Deterministic event sequences produce identical scores on repeated evaluation; score version is recorded.
- Stored sessions survive refresh; noncomparable sessions are not shown as a direct score trend.
- Responsive UI, keyboard controls, browser-permission denied state, no-device state, no-audio-unlock state, broken import state.

**Do not mark the project complete if the landing screen looks polished but playback, imports, scoring, or history are stubs.** Likewise, don't mark real-hardware acceptance “passed” unless an actual USB MIDI device was tested.

### D. Library integration gate (must pass before marking v3 done)
1. Run the notation spike using the **actual installed version** of alphaTab: import `.mid` through Rust, render correctly beamed percussion notation, align the R/L lyric labels, follow the audio clock and overlay per-note feedback. If this fails, document the failed test and repeat with VexFlow; do not claim alphaTab integration merely because a score screenshot renders.
2. Prove that a count-in, click, and acoustic snare share the same audio clock; perform pause/resume, restart, tempo change between runs and loop tests. No doubling, accumulated phase drift, stale callbacks or cursor drift.
3. Simulate browser MIDI Note On/Off, CC traffic, input hotplug and denied permissions. Inspect original timestamp/byte preservation. Browser simulation **does not** count as USB hardware acceptance.
4. Verify that `midly` preserves lyric events and that ambiguous note/lyric associations are surfaced, not silently guessed. Test import of a MIDI fixture with two simultaneous snare notes.
5. Verify imported file bytes, mapping edits, preferences and session history survive reload. After schema version changes, old sessions remain readable.
6. Verify third-party licences, alphaTab attribution if selected, bundled metronome/snare audio provenance, pinned versions, and working production static build.

**Evidence quality:** The final Codex report must link each gate to an automated test, browser observation, screenshot or clear manual reproduction steps. Any gate not exercised must be marked unverified, with no implied pass.

## 11. Implementation plan and working method for Codex

Build vertical slices, checking the running browser after each. Complete the short, executable Dependency Selection Gate (Section 1A) while establishing Slice 1; do not overinvest in paper research:

**Slice 1 — runnable and delightful:** basic app shell, bundled paradiddle, genuinely rendered score and R/L, web audio snare/metronome, count-in, usable demo input, live feedback. Prove a complete practice loop before adding configuration complexity.

**Slice 2 — real MIDI and import:** permissions, device list, mapping/diagnostics, file import/parser, lyric interpretation, robust notation, explicit warning states.

**Slice 3 — quality and completeness:** hit matching/scorer, local session persistence/comparison, tempo/meter/subdivision controls, calibration, looping and autoscroll, test hardening, mobile layout and polish.

Before claiming completion:
- Build and run tests; fix errors and warnings you can resolve.
- Open the actual app and inspect screens visually if tools permit. Exercise normal, no-device, connected, demo, count-in, paused, completed, error states.
- Verify at least one **real end-to-end demo run** through the same pipeline as MIDI.
- Provide exact run commands and a factual completion matrix of P01–P12: implemented/tested, implemented/not hardware-tested, partial, or not done.
- State any remaining gaps clearly; never disguise them with optimistic wording.

**Decision rule:** where requirements conflict, choose correctness and an intuitive end-to-end experience over completeness of obscure MIDI formats or architectural purity. Keep code legible enough to test and debug despite AI being the main maintainer. Never optimise away safeguards or instrumentation.

## 12. Source/reference starting points (verify at implementation time)

- Web MIDI API / permissions: https://developer.mozilla.org/en-US/docs/Web/API/Web_MIDI_API
- MIDI event data and timestamps: https://developer.mozilla.org/en-US/docs/Web/API/MIDIMessageEvent
- Web Audio `currentTime` clock: https://developer.mozilla.org/en-US/docs/Web/API/BaseAudioContext/currentTime
- alphaTab documentation (percussion, programmatic scores, external cursor): https://docs.alphatab.net/docs/features/ ; https://next.alphatab.net/docs/features/extend/programmatic-score-construction ; https://www.alphatab.net/docs/releases/release1_6/
- alphaTab API and licensing: https://alphatab.net/docs/reference/types/synth/iexternalmediasynthoutput ; https://www.alphatab.net/docs/attribution ; https://www.npmjs.com/package/@coderline/alphatab
- VexFlow engraving fallback: https://github.com/vexflow/vexflow
- Tone.js audio: https://tonejs.github.io/ ; https://github.com/tonejs/tone.js/wiki/Accurate-Timing
- WEBMIDI.js: https://webmidijs.org/docs/ ; https://webmidijs.org/api/classes/Input/
- Rust IndexedDB wrappers: https://docs.rs/rexie/latest/rexie/ ; https://docs.rs/idb/latest/idb/
- JS/Rust integration: https://docs.rs/serde-wasm-bindgen/latest/serde_wasm_bindgen/
- Virtual Web MIDI tests: https://github.com/jazz-soft/JZZ
- Framework-independent CSS component option: https://daisyui.com/docs/intro/
- Rust `midly`/MIDI lyric events: https://docs.rs/midly/latest/midly/enum.MetaMessage.html
- EarDrum official product and feature benchmark: https://ear-drum.app/
- EarDrum App Store release notes documenting lyric-event sticking and scrolling/calibration improvements: https://apps.apple.com/gb/app/ear-drum-practice-rudiments/id6755543411

## 13. Final instruction

**Implement the application, not the idea of an application.** Assume a demanding user will open the PoC immediately and judge whether they can meaningfully practise drums. A good PoC is a tiny but complete musical experience: easy to understand, beautiful to look at, satisfying to play, and trustworthy in its feedback. Continue until that first session works, or report a specific blocker and the exact scope actually completed.
