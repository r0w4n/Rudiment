# Optional presentation integration

`rudiment-core` owns MIDI parsing, expected hits and scoring. Leptos owns the
exercise, selected bar sequence, practice state and live judgements. Tone's
existing transport and animation frame remain the sole source of playback
position. VexFlow notation continues on its existing path.

The optional presentation host bridges those existing sources to
`web/js/highway-controller.mjs`. It asks Leptos for a snapshot only when a
renderer mounts or the exercise changes. While mounted, existing Leptos state
effects forward configuration and judgement changes; the existing Tone frame
forwards its beat and effective tempo. When no renderer is mounted, those
effects do not serialize presentation data and the frame does not construct a
presentation. There is no additional clock, frame loop, event listener or
exercise/scoring state machine.

The controller projects the selected bars, including noncontiguous bars and
repetitions, into a beat-based visible range. The immutable snapshot contains
lanes, note IDs, source and sequence positions, durations, sticking, accents, grace flags,
bar/beat boundaries, playback phase and tempo, selection and loop iteration,
and the app's supplied judgements. Bar membership uses the same written-onset
rule as Rust's expanded sequence; note timing still uses the original MIDI
onset. Rust supplies the matched repetition from the actual scored expected ID,
so early hits belonging to the next loop need not be guessed by a renderer.

## SVG Highway renderer

A renderer implementation can call
`rudiment.mountPresentationRenderer(renderer, lanes)` after app startup. The
renderer supplies `initialize()`, `update(presentation)`, `resize(viewport)`
and `dispose()`. The returned handle forwards `resize(viewport)` and `dispose()`;
the caller owns any viewport observer or rendering resources it creates. Only
one optional renderer may be mounted at a time. Disposing it neither stops
Tone nor affects notation, MIDI or scoring; app cleanup also unbinds it. A
renderer failure is isolated and detaches the renderer rather than interrupting
the transport. The renderer is not passed playback or scoring controls.

The default configuration has kick, snare and hi-hat lanes. Closed, pedal and
open hi-hat events share the hi-hat lane while retaining their articulation.
Mounting against other unsupported instruments still fails explicitly. If a
later exercise is unsupported, the optional renderer is detached. Additional
lanes require configuration and rendering support, not another MIDI parser or
clock.

The built-in SVG Highway mounts only when selected and uses the existing host.
It draws bounded, keyed note and grid elements under a fixed centred playhead;
resize observation is owned and disconnected by the mount. When idle, the
timeline rests at the configured count-in start for the selected bars, avoiding
a jump on Start. Expected notes use a transparent interior and white outline;
matched notes fill with the same Perfect, Good, Early and Late colours as
Notation. R/L sticking and accents are unchanged, and accessible labels expose
the assessment without adding a second hit symbol. Scoring policy v4 supplies
provisional hit feedback, provisional/confirmed invalid strikes and confirmed
misses from the authoritative Rust engine. The renderer never expires or
classifies a note itself. Confirmed misses retain their stable expected ID and
explicit loop iteration for scoring and accessibility, but remain visually the
same white outline as an unplayed expected note. Extra and Wrong Instrument
strikes render as small red dots at their actual beat on the instrument lane;
matched hits never create a second marker. Notation consumes the same
replacement snapshot, including provisional corrections and restart clears.
Loop transitions clear only this renderer projection; the scorer keeps the
confirmed iteration results. The projection also bounds visually obsolete
unmatched-hit markers while final scoring retains every event. See
[`scoring-policy-v4.md`](scoring-policy-v4.md) for the matching, event-time and
late-drain rules. Simulated browser tests do not establish physical MIDI or
audible latency.
