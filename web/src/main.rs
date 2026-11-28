use js_sys::{Array, Date, Function, Promise, Uint8Array};
use leptos::prelude::*;
use lucide_leptos::{
    ArrowLeft, ArrowRight, ChartNoAxesCombined, FolderOpen, Info, Moon, Pause, Play, RotateCcw,
    Settings as SettingsIcon, Star, Sun, X,
};
use rudiment_core::adaptive::{
    AdaptiveAction, AdaptiveAttempt, AdaptiveController, AdaptiveDecision, AdaptivePhase,
    AdaptiveSettings,
};
use rudiment_core::{
    Articulation, DEFAULT_GOOD_MS, DEFAULT_MATCH_MS, DEFAULT_PERFECT_MS, Exercise, ExpectedHit,
    IncrementalScorer, InputSource, Instrument, Judgement, MAX_MIDI_FILE_BYTES, MidiBinding,
    ObservedHit, PerformanceAward, PerformanceRating, SCORING_VERSION, ScoringConfig,
    ScoringFeedback, SessionSummary, TempoChange, beat_at_elapsed_ms, classify_midi,
    elapsed_ms_at_beat, paradiddle, parse_midi, performance_rating, summary_from_results,
};
#[cfg(test)]
use rudiment_core::{HitResult, score_session_with_tempo_map};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
    sync::Arc,
};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::{JsFuture, spawn_local};

type FrameCallback = Closure<dyn FnMut(f64, f64, f64)>;
type DoneCallback = Closure<dyn FnMut()>;
type CallbackSlot<T> = Rc<RefCell<Option<T>>>;
type LaunchRun = Rc<dyn Fn(f64, f64, bool)>;
type LaunchSlot = Rc<RefCell<Option<LaunchRun>>>;
type StartRun = Rc<dyn Fn(u8)>;
type StartSlot = CallbackSlot<StartRun>;

thread_local! {
    static ADAPTIVE_SAVE: RefCell<Option<Rc<dyn Fn()>>> = RefCell::new(None);
}

fn save_adaptive_settings() {
    if let Some(save) = ADAPTIVE_SAVE.with(|cell| cell.borrow().clone()) {
        save();
    }
}
type HitInput = Rc<dyn Fn(f64, Instrument, Articulation, u8, InputSource, Option<u8>, Option<u8>)>;
type MidiFileCallback = Closure<dyn FnMut(Uint8Array, String, String, JsValue)>;

#[derive(Clone, Copy)]
struct BuiltinRudiment {
    id: &'static str,
    title: &'static str,
    description: &'static str,
    difficulty: Difficulty,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Difficulty {
    Beginner,
    Intermediate,
    Advanced,
}

impl Difficulty {
    fn label(self) -> &'static str {
        match self {
            Self::Beginner => "Beginner",
            Self::Intermediate => "Intermediate",
            Self::Advanced => "Advanced",
        }
    }
}

fn parse_difficulty_label(label: &str) -> Option<Difficulty> {
    match label {
        "Beginner" => Some(Difficulty::Beginner),
        "Intermediate" => Some(Difficulty::Intermediate),
        "Advanced" => Some(Difficulty::Advanced),
        _ => None,
    }
}

const BUILTIN_RUDIMENTS: [BuiltinRudiment; 13] = [
    BuiltinRudiment {
        id: "01_roll",
        title: "Roll",
        description: "Alternating double strokes; long-roll mechanism.",
        difficulty: Difficulty::Intermediate,
    },
    BuiltinRudiment {
        id: "02_mother_or_5_s_roll",
        title: "Mother or 5 S. Roll",
        description: "Four internal strokes followed by an accented release.",
        difficulty: Difficulty::Intermediate,
    },
    BuiltinRudiment {
        id: "03_7_stroke_roll",
        title: "7 Stroke Roll",
        description: "Three doubles and an accented finishing tap.",
        difficulty: Difficulty::Intermediate,
    },
    BuiltinRudiment {
        id: "04_9_stroke_roll",
        title: "9 Stroke Roll",
        description: "Four doubles and an accented sustained finishing tap.",
        difficulty: Difficulty::Intermediate,
    },
    BuiltinRudiment {
        id: "05_hard_flams",
        title: "Hard Flams",
        description: "Alternating flams with sounded grace notes.",
        difficulty: Difficulty::Intermediate,
    },
    BuiltinRudiment {
        id: "06_single_paradiddle",
        title: "Single Paradiddle",
        description: "RLRR / LRLL with four-note accents.",
        difficulty: Difficulty::Intermediate,
    },
    BuiltinRudiment {
        id: "07_double_paradiddle",
        title: "Double Paradiddle",
        description: "RLRLRR / LRLRLL in eighth-note triplets.",
        difficulty: Difficulty::Intermediate,
    },
    BuiltinRudiment {
        id: "08_flam_paradiddle",
        title: "Flam Paradiddle",
        description: "Paradiddles with opposing-hand flam grace strokes.",
        difficulty: Difficulty::Advanced,
    },
    BuiltinRudiment {
        id: "09_single_drag",
        title: "Single Drag",
        description: "Two opposing-hand grace strokes before each tap.",
        difficulty: Difficulty::Advanced,
    },
    BuiltinRudiment {
        id: "10_single_rotamacue",
        title: "Single Rotamacue",
        description: "Drag and triplet cells using the historical spelling.",
        difficulty: Difficulty::Advanced,
    },
    BuiltinRudiment {
        id: "11_advanced_the_drummers_call_study",
        title: "The Drummers Call — Advanced Study",
        description: "An original eight-bar study inspired by the historical call.",
        difficulty: Difficulty::Advanced,
    },
    BuiltinRudiment {
        id: "12_beginner_single_strokes",
        title: "Single Strokes — Beginner",
        description: "Two bars of alternating quarter notes.",
        difficulty: Difficulty::Beginner,
    },
    BuiltinRudiment {
        id: "13_beginner_paradiddle",
        title: "Single Paradiddle — Beginner",
        description: "Two short bars of RLRR / LRLL in eighth notes.",
        difficulty: Difficulty::Beginner,
    },
];

fn builtin_count(difficulty: Option<Difficulty>) -> usize {
    BUILTIN_RUDIMENTS
        .iter()
        .filter(|item| difficulty.is_none_or(|level| item.difficulty == level))
        .count()
}

const LIBRARY_PAGE_SIZE: usize = 6;
const CONTINUOUS_LOOP_CHUNK: u8 = 240;
const RESULT_DISMISS_GRACE_MS: f64 = 500.0;
const SCORING_DELIVERY_GRACE_MS: f64 = 32.0;
const SCORING_TAIL_MS: f64 = 2.0 * DEFAULT_MATCH_MS + SCORING_DELIVERY_GRACE_MS + 2.0;
const LIVE_UNMATCHED_FEEDBACK_LIMIT: usize = 256;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
enum AppMode {
    #[default]
    Practice,
    Performance,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
enum RepeatTolerance {
    Relaxed,
    #[default]
    Standard,
    Strict,
}
// Runtime state of the bridge's session-only diagnostic switches, seeded
// once on mount; camelCase matches the bridge's `diagnosticsSnapshot()` keys.
#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct DiagnosticsSnapshot {
    reference_overlay: bool,
    visual_feedback: bool,
    metronome_visual: bool,
    metronome_needle: bool,
    metronome_pulses: bool,
    state_tick: bool,
    auto_scroll: bool,
    perf_counters: bool,
}
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = rudiment, js_name = ensureAudio)]
    fn ensure_audio() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = previewClick)]
    fn preview_click(timbre: &str) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = playDrum)]
    fn play_drum(instrument: &str, articulation: &str, velocity: u8) -> bool;
    #[wasm_bindgen(js_namespace = rudiment, js_name = setVolumes)]
    fn set_volumes(snare: f64, click: f64);
    #[wasm_bindgen(js_namespace = rudiment, js_name = startAudio)]
    fn start_audio(options: JsValue, frame: &Function, done: &Function) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = setAudioTempo)]
    fn set_audio_tempo(bpm: f64) -> bool;
    #[wasm_bindgen(js_namespace = rudiment, js_name = stopAudio)]
    fn stop_audio();
    #[wasm_bindgen(js_namespace = rudiment, js_name = currentAudioBeat)]
    fn current_audio_beat() -> f64;
    #[wasm_bindgen(js_namespace = rudiment, js_name = audioOutputRunning)]
    fn audio_output_running() -> bool;
    #[wasm_bindgen(js_namespace = rudiment, js_name = bindPresentationSnapshot)]
    fn bind_presentation_snapshot(callback: &Function);
    #[wasm_bindgen(js_namespace = rudiment, js_name = unbindPresentation)]
    fn unbind_presentation();
    #[wasm_bindgen(js_namespace = rudiment, js_name = presentationActive)]
    fn presentation_active() -> bool;
    #[wasm_bindgen(js_namespace = rudiment, js_name = presentationExerciseChanged)]
    fn presentation_exercise_changed();
    #[wasm_bindgen(js_namespace = rudiment, js_name = presentationConfiguration)]
    fn presentation_configuration(value: JsValue);
    #[wasm_bindgen(js_namespace = rudiment, js_name = presentationJudgements)]
    fn presentation_judgements(value: JsValue);
    #[wasm_bindgen(js_namespace = rudiment, js_name = savedHighwayMode)]
    fn saved_highway_mode() -> bool;
    #[wasm_bindgen(js_namespace = rudiment, js_name = saveHighwayMode)]
    fn save_highway_mode(selected: bool);
    #[wasm_bindgen(js_namespace = rudiment, js_name = setHighwayMode)]
    fn set_highway_mode(selected: bool);
    #[wasm_bindgen(js_namespace = rudiment, js_name = savedNotationTheme)]
    fn saved_notation_theme() -> bool;
    #[wasm_bindgen(js_namespace = rudiment, js_name = saveNotationTheme)]
    fn save_notation_theme(dark: bool);
    #[wasm_bindgen(js_namespace = rudiment, js_name = savedOnboardingSkip)]
    fn saved_onboarding_skip() -> bool;
    #[wasm_bindgen(js_namespace = rudiment, js_name = saveOnboardingSkip)]
    fn save_onboarding_skip(skip: bool);
    #[wasm_bindgen(js_namespace = rudiment, js_name = startCalibration)]
    fn start_calibration() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = renderScore)]
    fn render_score(exercise: JsValue);
    #[wasm_bindgen(js_namespace = rudiment, js_name = setSelectedBars)]
    fn set_selected_bars(bars: JsValue);
    #[wasm_bindgen(js_namespace = rudiment, js_name = bindScoreSelection)]
    fn bind_score_selection(callback: &Function);
    #[wasm_bindgen(js_namespace = rudiment, js_name = scrollScoreToBeat)]
    fn scroll_score_to_beat(beat: f64);
    #[wasm_bindgen(js_namespace = rudiment, js_name = queueScoreFeedback)]
    fn queue_score_feedback(judgements: JsValue);
    #[wasm_bindgen(js_namespace = rudiment, js_name = flushScoreFeedback)]
    fn flush_score_feedback();
    #[wasm_bindgen(js_namespace = rudiment, js_name = clearScoreFeedback)]
    fn clear_score_feedback();
    #[wasm_bindgen(js_namespace = rudiment, js_name = clearScoreFeedbackPreserveViewport)]
    fn clear_score_feedback_preserve_viewport();
    #[wasm_bindgen(js_namespace = rudiment, js_name = connectMidi)]
    fn connect_midi(callback: &Function) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = autoConnectMidi)]
    fn auto_connect_midi(callback: &Function) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = selectMidi)]
    fn select_midi(id: &str);
    #[wasm_bindgen(js_namespace = rudiment, js_name = chooseMidi)]
    fn choose_midi(callback: &Function, max_file_bytes: usize);
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadBundled)]
    fn load_bundled(url: &str, callback: &Function, save_to_library: bool, display_name: &str);
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadBundledFromLibrary)]
    fn load_bundled_from_library(url: &str, display_name: &str);
    #[wasm_bindgen(js_namespace = rudiment, js_name = bindDrop)]
    fn bind_drop(callback: &Function, max_file_bytes: usize);
    #[wasm_bindgen(js_namespace = rudiment, js_name = trapDialogTab)]
    fn trap_dialog_tab(event: &web_sys::KeyboardEvent, selector: &str);
    #[wasm_bindgen(js_namespace = rudiment, js_name = saveSession)]
    fn save_session(value: JsValue) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadSessions)]
    fn load_sessions() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = clearHistory)]
    fn clear_history() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = setPracticeActive)]
    fn set_practice_active(active: bool);
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadPracticeTime)]
    fn load_practice_time() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = exportHistory)]
    fn export_history() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = saveSettings)]
    fn save_settings(value: JsValue) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadSettings)]
    fn load_settings() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = saveProfile)]
    fn save_profile(value: JsValue) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadProfiles)]
    fn load_profiles() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = resetLocalSetup)]
    fn reset_local_setup() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = setDiagnostic)]
    fn set_diagnostic(name: &str, value: bool);
    #[wasm_bindgen(js_namespace = rudiment, js_name = diagnosticsSnapshot)]
    fn diagnostics_snapshot() -> JsValue;
    #[wasm_bindgen(js_namespace = rudiment, js_name = bridgeBuild)]
    fn bridge_build() -> String;
    #[wasm_bindgen(js_namespace = rudiment, js_name = saveImport)]
    fn save_import(hash: &str, name: &str, bytes: &Uint8Array, metadata: JsValue) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadImports)]
    fn load_imports() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadImport)]
    fn load_import(hash: &str) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = deleteImport)]
    fn delete_import(hash: &str) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadSavedImport)]
    fn load_saved_import(hash: &str) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = saveLastImport)]
    fn save_last_import(hash: JsValue) -> Promise;
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct Settings {
    mode: Option<AppMode>,
    bpm: Option<f64>,
    count_in_bars: Option<u8>,
    subdivision: Option<u8>,
    timbre: Option<String>,
    metronome: Option<bool>,
    click_volume: Option<f64>,
    snare_volume: Option<f64>,
    snare_muted: Option<bool>,
    snare_reference_mode: Option<String>,
    loops: Option<u8>,
    repeat_bar_until_good: Option<bool>,
    repeat_tolerance: Option<RepeatTolerance>,
    selection_start_bar: Option<usize>,
    selection_end_bar: Option<usize>,
    selected_bars: Option<Vec<usize>>,
    latency_ms: Option<f64>,
    on_time_ms: Option<f64>,
    excellent_ms: Option<f64>,
    snare_note: Option<u8>,
    snare_channel: Option<u8>,
    midi_bindings: Option<Vec<MidiBinding>>,
    welcome_dismissed: Option<bool>,
    last_import_hash: Option<String>,
    auto_show_statistics: Option<bool>,
    adaptive_enabled: Option<bool>,
    adaptive: Option<AdaptiveSettings>,
    library_difficulty: Option<String>,
    selected_track: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
struct AdaptiveStatus {
    phase: AdaptivePhase,
    action: AdaptiveAction,
    bpm: f64,
    target_bpm: f64,
    bars: Vec<usize>,
    stars: u8,
    explanation: String,
    completed: bool,
}

impl AdaptiveStatus {
    fn from_decision(decision: &AdaptiveDecision) -> Self {
        Self {
            phase: decision.phase,
            action: decision.action,
            bpm: decision.bpm,
            target_bpm: decision.target_bpm,
            bars: decision.bars.clone(),
            stars: decision.stars,
            explanation: decision.explanation.clone(),
            completed: decision.completed,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedMidiImport {
    hash: String,
    name: String,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ReferenceDrumNote {
    beat: f64,
    velocity: u8,
    instrument: Instrument,
    articulation: Articulation,
}

fn reference_drum_notes(
    expected: &[ExpectedHit],
    start_beat: f64,
    end_beat: f64,
) -> Vec<ReferenceDrumNote> {
    let mut notes = expected
        .iter()
        .filter(|hit| hit.beat >= start_beat - 0.000_001 && hit.beat < end_beat - 0.000_001)
        .map(|hit| ReferenceDrumNote {
            beat: hit.beat,
            velocity: hit.velocity.max(1),
            instrument: hit.instrument,
            articulation: hit.articulation,
        })
        .collect::<Vec<_>>();
    notes.sort_by(|a, b| {
        a.beat
            .total_cmp(&b.beat)
            .then_with(|| (a.instrument as u8).cmp(&(b.instrument as u8)))
            .then_with(|| (a.articulation as u8).cmp(&(b.articulation as u8)))
    });
    notes.dedup_by(|later, earlier| {
        if (later.beat - earlier.beat).abs() < 0.000_001
            && later.instrument == earlier.instrument
            && later.articulation == earlier.articulation
        {
            earlier.velocity = earlier.velocity.max(later.velocity);
            true
        } else {
            false
        }
    });
    notes
}

fn default_midi_bindings() -> Vec<MidiBinding> {
    vec![
        MidiBinding {
            note: 36,
            channel: 10,
            instrument: Instrument::Kick,
            articulation: Articulation::Center,
        },
        MidiBinding {
            note: 38,
            channel: 10,
            instrument: Instrument::Snare,
            articulation: Articulation::Center,
        },
        MidiBinding {
            note: 42,
            channel: 10,
            instrument: Instrument::ClosedHat,
            articulation: Articulation::Closed,
        },
        MidiBinding {
            note: 46,
            channel: 10,
            instrument: Instrument::OpenHat,
            articulation: Articulation::Open,
        },
    ]
}

fn learned_articulation(instrument: Instrument) -> Articulation {
    match instrument {
        Instrument::Kick | Instrument::Snare => Articulation::Center,
        Instrument::ClosedHat | Instrument::PedalHat => Articulation::Closed,
        Instrument::OpenHat => Articulation::Open,
        _ => Articulation::Unspecified,
    }
}

fn instrument_label(instrument: Instrument) -> &'static str {
    match instrument {
        Instrument::Kick => "kick",
        Instrument::Snare => "snare",
        Instrument::ClosedHat | Instrument::PedalHat => "closed hi-hat",
        Instrument::OpenHat => "open hi-hat",
        _ => "drum",
    }
}

fn instrument_key(instrument: Instrument) -> &'static str {
    match instrument {
        Instrument::Kick => "kick",
        Instrument::Snare => "snare",
        Instrument::ClosedHat => "closed_hat",
        Instrument::OpenHat => "open_hat",
        Instrument::PedalHat => "pedal_hat",
        Instrument::Ride => "ride",
        Instrument::Crash => "crash",
        Instrument::Tom => "tom",
        Instrument::OtherPercussion => "other_percussion",
        Instrument::Unknown => "unknown",
    }
}

fn articulation_key(articulation: Articulation) -> &'static str {
    match articulation {
        Articulation::Unspecified => "unspecified",
        Articulation::Center => "center",
        Articulation::Rim => "rim",
        Articulation::Closed => "closed",
        Articulation::Open => "open",
        Articulation::Pedal => "pedal",
        Articulation::SnareWiresOn => "snare_wires_on",
        Articulation::SnareWiresOff => "snare_wires_off",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionRecord {
    id: String,
    ended_at: String,
    exercise_id: String,
    exercise_title: String,
    #[serde(default)]
    exercise_hash: String,
    bpm: f64,
    input_mode: InputSource,
    scoring_version: String,
    tolerance_ms: f64,
    latency_ms: f64,
    summary: SessionSummary,
    #[serde(default)]
    observed: Vec<ObservedHit>,
    #[serde(default)]
    schema_version: u8,
    #[serde(default)]
    mode: AppMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MidiPort {
    id: String,
    name: String,
    manufacturer: String,
    state: String,
    connection: String,
}

/// A MIDI connection counts as established only when the selected port is
/// physically present and open. This single predicate decides whether a
/// MIDI-dependent activity (like mapping) reopens the connection modal.
fn midi_connection_ready(ports: &[MidiPort], selected: Option<&str>) -> bool {
    selected.is_some_and(|id| {
        ports
            .iter()
            .any(|port| port.id == id && port.state == "connected" && port.connection == "open")
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct MidiEnvelope {
    kind: String,
    #[serde(default)]
    ports: Vec<MidiPort>,
    selected_id: Option<String>,
    permission: Option<String>,
    error: Option<String>,
    #[serde(rename = "type")]
    message_type: Option<String>,
    note: Option<u8>,
    velocity: Option<u8>,
    channel: Option<u8>,
    timestamp: Option<f64>,
    #[serde(default)]
    raw_bytes: Vec<u8>,
    input_id: Option<String>,
    input_name: Option<String>,
    disconnected_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct SavedMidiProfile {
    id: String,
    #[serde(default)]
    bindings: Vec<MidiBinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct LiveJudgement {
    expected_id: Option<String>,
    #[serde(default)]
    iteration: Option<usize>,
    expected_beat: Option<f64>,
    actual_beat: Option<f64>,
    instrument: Instrument,
    judgement: Judgement,
    error_ms: f64,
    excellent: bool,
    note_score: f64,
    event_id: Option<String>,
    #[serde(default)]
    confirmed: bool,
}

fn beats_per_bar(exercise: &Exercise) -> f64 {
    exercise.meter_numerator as f64 * 4.0 / exercise.meter_denominator.max(1) as f64
}

fn format_practice_time(milliseconds: f64) -> String {
    let minutes = (milliseconds.max(0.0) / 60_000.0).floor() as u64;
    let hours = minutes / 60;
    let remaining = minutes % 60;
    match (hours, remaining) {
        (0, 1) => "1 minute".into(),
        (0, _) => format!("{remaining} minutes"),
        (1, 0) => "1 hour".into(),
        (_, 0) => format!("{hours} hours"),
        (1, 1) => "1 hour 1 minute".into(),
        (1, _) => format!("1 hour {remaining} minutes"),
        (_, 1) => format!("{hours} hours 1 minute"),
        _ => format!("{hours} hours {remaining} minutes"),
    }
}

fn timing_hit_counts(summary: &SessionSummary) -> (usize, usize, usize, usize) {
    let perfect = summary
        .results
        .iter()
        .filter(|hit| hit.excellent || hit.judgement == Judgement::Perfect)
        .count();
    let good = summary
        .results
        .iter()
        .filter(|hit| {
            hit.judgement == Judgement::Good
                || (hit.judgement == Judgement::OnTime && !hit.excellent)
        })
        .count();
    let early = summary
        .results
        .iter()
        .filter(|hit| hit.judgement == Judgement::Early)
        .count();
    let late = summary
        .results
        .iter()
        .filter(|hit| hit.judgement == Judgement::Late)
        .count();
    (perfect, good, early, late)
}

fn percentage(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        100.0 * count as f64 / total as f64
    }
}

fn display_score(score: Option<f64>) -> String {
    score
        .map(|value| format!("{value:.0}"))
        .unwrap_or_else(|| "—".into())
}

fn is_new_high_score(current: &SessionRecord, history: &[SessionRecord]) -> bool {
    let Some(score) = current.summary.score else {
        return false;
    };
    history
        .iter()
        .filter(|session| {
            session.id != current.id
                && session.exercise_hash == current.exercise_hash
                && session.scoring_version == current.scoring_version
        })
        .filter_map(|session| session.summary.score)
        .max_by(f64::total_cmp)
        .is_some_and(|previous_best| score > previous_best)
}

fn is_white_mastery(summary: &SessionSummary) -> bool {
    let expected_results = summary
        .results
        .iter()
        .filter(|result| result.expected_id.is_some())
        .collect::<Vec<_>>();
    summary.expected > 0
        && expected_results.len() == summary.expected
        && summary.missed == 0
        && summary.extras == 0
        && summary.wrong_instrument == 0
        && expected_results.iter().all(|result| {
            matches!(
                result.judgement,
                Judgement::Perfect | Judgement::Good | Judgement::OnTime
            )
        })
}

fn achievement_copy(summary: &SessionSummary) -> (&'static str, &'static str) {
    if is_white_mastery(summary) {
        ("Mastered!", "Every expected note landed cleanly.")
    } else {
        match performance_rating(summary).stars {
            3 => (
                "Exercise complete!",
                "Strong work — keep refining the details.",
            ),
            2 => ("Good job!", "You're on the right track. Keep it up!"),
            1 => (
                "Good start!",
                "Keep building consistency one run at a time.",
            ),
            _ => ("Keep going!", "Every focused attempt moves you forward."),
        }
    }
}

fn same_exercise(left: &SessionRecord, right: &SessionRecord) -> bool {
    if !left.exercise_hash.is_empty() && !right.exercise_hash.is_empty() {
        left.exercise_hash == right.exercise_hash
    } else {
        !left.exercise_id.is_empty() && left.exercise_id == right.exercise_id
    }
}

fn progress_sessions(current: &SessionRecord, history: &[SessionRecord]) -> Vec<(String, f64)> {
    let mut matching = history
        .iter()
        .filter(|session| {
            same_exercise(session, current)
                && session.summary.score.is_some()
                && (session.ended_at.as_str(), session.id.as_str())
                    <= (current.ended_at.as_str(), current.id.as_str())
        })
        .collect::<Vec<_>>();
    matching.sort_by(|left, right| {
        left.ended_at
            .cmp(&right.ended_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut scores = matching
        .into_iter()
        .filter_map(|session| {
            session
                .summary
                .score
                .map(|score| (session.id.clone(), score))
        })
        .collect::<Vec<_>>();
    if !history.iter().any(|session| session.id == current.id)
        && let Some(score) = current.summary.score
    {
        scores.push((current.id.clone(), score));
    }
    scores
}

fn progress_chart_points(scores: &[f64]) -> Vec<(f64, f64)> {
    const LEFT: f64 = 48.0;
    const RIGHT: f64 = 712.0;
    const TOP: f64 = 14.0;
    const BOTTOM: f64 = 146.0;
    scores
        .iter()
        .enumerate()
        .map(|(index, score)| {
            let x = if scores.len() <= 1 {
                (LEFT + RIGHT) / 2.0
            } else {
                LEFT + (RIGHT - LEFT) * index as f64 / (scores.len() - 1) as f64
            };
            let y = BOTTOM - (BOTTOM - TOP) * score.clamp(0.0, 100.0) / 100.0;
            (x, y)
        })
        .collect()
}

fn exercise_beats(exercise: &Exercise) -> f64 {
    let last = exercise.hits.iter().map(|hit| hit.beat).fold(0.0, f64::max);
    let bar_beats = beats_per_bar(exercise);
    ((last + 0.001) / bar_beats).ceil().max(1.0) * bar_beats
}

#[derive(Debug, Clone, PartialEq)]
struct BarSequence {
    bars: Vec<usize>,
    bar_beats: f64,
    origin: f64,
    total_bars: usize,
}

impl BarSequence {
    fn new(exercise: &Exercise, selected: &[usize]) -> Self {
        let bar_beats = beats_per_bar(exercise);
        let count = (exercise_beats(exercise) / bar_beats).ceil().max(1.0) as usize;
        let mut bars = selected
            .iter()
            .copied()
            .filter(|bar| *bar < count)
            .collect::<Vec<_>>();
        bars.sort_unstable();
        bars.dedup();
        if bars.is_empty() {
            bars = (0..count).collect();
        }
        let origin = bars[0] as f64 * bar_beats;
        Self {
            bars,
            bar_beats,
            origin,
            total_bars: count,
        }
    }

    fn length(&self) -> f64 {
        self.bars.len() as f64 * self.bar_beats
    }

    fn display_beat(&self, beat: f64) -> f64 {
        let within = (beat - self.origin).rem_euclid(self.length());
        let index = ((within / self.bar_beats).floor() as usize).min(self.bars.len() - 1);
        self.bars[index] as f64 * self.bar_beats + within - index as f64 * self.bar_beats
    }
}

fn expected_notes_per_bar(exercise: &Exercise) -> Vec<usize> {
    let bar_beats = beats_per_bar(exercise);
    let total_bars = (exercise_beats(exercise) / bar_beats).ceil().max(1.0) as usize;
    let mut counts = vec![0usize; total_bars];
    for hit in &exercise.hits {
        if let Some(count) = counts.get_mut(source_bar(hit, bar_beats)) {
            *count += 1;
        }
    }
    counts
}

fn legacy_selected_bars(exercise: &Exercise, start: usize, end: usize) -> Vec<usize> {
    let count = (exercise_beats(exercise) / beats_per_bar(exercise))
        .ceil()
        .max(1.0) as usize;
    let start = start.min(count - 1);
    let end = end.min(count - 1).max(start);
    if start == 0 && end + 1 == count {
        Vec::new()
    } else {
        (start..=end).collect()
    }
}

fn expanded_tempo_map(exercise: &Exercise, loops: u8) -> Vec<TempoChange> {
    let length = exercise_beats(exercise);
    (0..loops)
        .flat_map(|repetition| {
            exercise
                .tempo_map
                .iter()
                .filter(|change| change.beat < length)
                .cloned()
                .map(move |mut change| {
                    change.beat += repetition as f64 * length;
                    change.tick += (repetition as f64 * length * exercise.ticks_per_beat as f64)
                        .round() as u64;
                    change
                })
        })
        .collect()
}

fn scaled_tempo_map(exercise: &Exercise, loops: u8, bpm: f64) -> Vec<TempoChange> {
    let scale = bpm / exercise.tempo_bpm.max(1.0);
    expanded_tempo_map(exercise, loops)
        .into_iter()
        .map(|mut change| {
            change.bpm *= scale;
            change
        })
        .collect()
}

fn expanded_sequence_tempo_map(
    exercise: &Exercise,
    sequence: &BarSequence,
    loops: u8,
) -> Vec<TempoChange> {
    let mut result = Vec::new();
    for repetition in 0..loops {
        for (index, bar) in sequence.bars.iter().enumerate() {
            let source_start = *bar as f64 * sequence.bar_beats;
            let timeline_start = sequence.origin
                + repetition as f64 * sequence.length()
                + index as f64 * sequence.bar_beats;
            let first_after = exercise
                .tempo_map
                .partition_point(|change| change.beat <= source_start + 0.000_001);
            let start_bpm = first_after
                .checked_sub(1)
                .map(|index| exercise.tempo_map[index].bpm)
                .unwrap_or(exercise.tempo_bpm);
            let changes = std::iter::once((timeline_start, start_bpm)).chain(
                exercise.tempo_map[first_after..]
                    .iter()
                    .take_while(|change| {
                        change.beat < source_start + sequence.bar_beats - 0.000_001
                    })
                    .map(|change| (timeline_start + change.beat - source_start, change.bpm)),
            );
            for (beat, bpm) in changes {
                if result
                    .last()
                    .is_none_or(|last: &TempoChange| (last.bpm - bpm).abs() > 0.000_001)
                {
                    result.push(TempoChange {
                        tick: (beat * exercise.ticks_per_beat as f64).round() as u64,
                        beat,
                        bpm,
                    });
                }
            }
        }
    }
    result
}

fn retime_tempo_map(
    existing: &[TempoChange],
    exercise: &Exercise,
    sequence: &BarSequence,
    loops: u8,
    beat: f64,
    bpm: f64,
) -> Vec<TempoChange> {
    let source = expanded_sequence_tempo_map(exercise, sequence, loops);
    let scale = bpm / exercise.tempo_bpm.max(1.0);
    let source_current = source
        .iter()
        .take_while(|change| change.beat <= beat + 0.000_001)
        .last()
        .map(|change| change.bpm)
        .unwrap_or(exercise.tempo_bpm);
    let mut result = existing
        .iter()
        .filter(|change| change.beat < beat - 0.000_001)
        .cloned()
        .collect::<Vec<_>>();
    result.push(TempoChange {
        tick: (beat.max(0.0) * exercise.ticks_per_beat as f64).round() as u64,
        beat: beat.max(0.0),
        bpm: source_current * scale,
    });
    result.extend(
        source
            .into_iter()
            .filter(|change| change.beat > beat + 0.000_001)
            .map(|mut change| {
                change.bpm *= scale;
                change
            }),
    );
    result
}

fn json_compatible<T: Serialize>(value: &T) -> Result<JsValue, serde_wasm_bindgen::Error> {
    value.serialize(&serde_wasm_bindgen::Serializer::json_compatible())
}

fn source_bar(hit: &ExpectedHit, bar_beats: f64) -> usize {
    ((hit.notation_beat + 0.000_001) / bar_beats)
        .floor()
        .max(0.0) as usize
}

#[derive(Serialize)]
struct PresentationHit<'a> {
    #[serde(flatten)]
    hit: &'a ExpectedHit,
    source_bar: usize,
}

#[derive(Serialize)]
struct PresentationExercise<'a> {
    meter_numerator: u8,
    meter_denominator: u8,
    ticks_per_beat: u16,
    hits: Vec<PresentationHit<'a>>,
}

fn presentation_exercise(exercise: &Exercise) -> PresentationExercise<'_> {
    let bar_beats = beats_per_bar(exercise);
    PresentationExercise {
        meter_numerator: exercise.meter_numerator,
        meter_denominator: exercise.meter_denominator,
        ticks_per_beat: exercise.ticks_per_beat,
        hits: exercise
            .hits
            .iter()
            .map(|hit| PresentationHit {
                hit,
                source_bar: source_bar(hit, bar_beats),
            })
            .collect(),
    }
}

fn expanded_sequence_hits(
    exercise: &Exercise,
    sequence: &BarSequence,
    loops: u8,
) -> Vec<ExpectedHit> {
    let mut source_by_bar = vec![Vec::new(); sequence.bars.last().copied().unwrap_or(0) + 1];
    for hit in &exercise.hits {
        let bar = source_bar(hit, sequence.bar_beats);
        if let Some(hits) = source_by_bar.get_mut(bar) {
            hits.push(hit);
        }
    }
    let mut result = Vec::new();
    for repetition in 0..loops {
        for (index, bar) in sequence.bars.iter().enumerate() {
            let source_start = *bar as f64 * sequence.bar_beats;
            let timeline_start = sequence.origin
                + repetition as f64 * sequence.length()
                + index as f64 * sequence.bar_beats;
            for hit in &source_by_bar[*bar] {
                let mut copy = (*hit).clone();
                copy.id = format!("{}-loop{}", copy.id, repetition + 1);
                copy.beat = timeline_start + hit.beat - source_start;
                result.push(copy);
            }
        }
    }
    result.sort_by(|left, right| left.beat.total_cmp(&right.beat));
    result
}

fn hits_in_bar(expected: &[ExpectedHit], start_beat: f64, end_beat: f64) -> Vec<ExpectedHit> {
    expected
        .iter()
        .filter(|hit| hit.beat >= start_beat && hit.beat < end_beat)
        .cloned()
        .collect()
}

fn repeat_bar_segment(offset: f64, bar_beats: f64, total_beats: f64) -> (f64, f64) {
    let start = (offset.max(0.0) / bar_beats).floor() * bar_beats;
    (start, (start + bar_beats).min(total_beats))
}

#[cfg(test)]
fn hits_for_bar_attempt(
    expected: &[ExpectedHit],
    start_beat: f64,
    bar_beats: f64,
) -> Vec<ExpectedHit> {
    hits_in_bar(expected, start_beat, start_beat + bar_beats)
}

fn rebase_hits_for_resume(hits: &mut [ObservedHit], previous_start_ms: f64, next_start_ms: f64) {
    let shift = next_start_ms - previous_start_ms;
    for hit in hits {
        hit.timestamp_ms += shift;
    }
}

fn session_start_ms_at_beat(
    start_ms: f64,
    offset: f64,
    bpm: f64,
    tempo_map: &[TempoChange],
) -> f64 {
    start_ms - elapsed_ms_at_beat(offset, bpm, tempo_map)
}

#[cfg(test)]
fn bar_attempt_is_good(
    expected_count: usize,
    matched_expected_count: usize,
    judgements: &[LiveJudgement],
    tolerance: RepeatTolerance,
) -> bool {
    matched_expected_count == expected_count
        && judgements.len() == expected_count
        && judgements.iter().all(|result| {
            result.expected_beat.is_some()
                && match tolerance {
                    RepeatTolerance::Relaxed => matches!(
                        result.judgement,
                        Judgement::Perfect | Judgement::Good | Judgement::Early | Judgement::Late
                    ),
                    RepeatTolerance::Standard => {
                        matches!(result.judgement, Judgement::Perfect | Judgement::Good)
                    }
                    RepeatTolerance::Strict => result.judgement == Judgement::Perfect,
                }
        })
}

#[cfg(test)]
fn include_failed_repeat_attempts(
    summary: &mut SessionSummary,
    attempts: &[(usize, Vec<LiveJudgement>)],
) {
    let mut score_sum = summary
        .results
        .iter()
        .filter(|result| result.expected_id.is_some())
        .map(|result| result.note_score)
        .sum::<f64>();
    let mut invalid_hits = summary.extras + summary.wrong_instrument;
    for (expected_count, judgements) in attempts {
        let matched = judgements
            .iter()
            .filter(|judgement| judgement.expected_id.is_some())
            .count();
        summary.expected += *expected_count;
        summary.matched += matched;
        summary.missed += expected_count.saturating_sub(matched);
        for judgement in judgements {
            score_sum += judgement.note_score;
            if judgement.expected_id.is_none() {
                invalid_hits += 1;
                if judgement.judgement == Judgement::WrongInstrument {
                    summary.wrong_instrument += 1;
                } else {
                    summary.extras += 1;
                }
            }
            summary.results.push(HitResult {
                expected_id: judgement.expected_id.clone(),
                expected_beat: judgement.expected_beat,
                expected_timestamp_ms: None,
                observed_ms: None,
                actual_timestamp_ms: None,
                error_ms: (judgement.expected_id.is_some()).then_some(judgement.error_ms),
                window_ms: DEFAULT_MATCH_MS,
                instrument: judgement.instrument,
                midi_note: None,
                judgement: judgement.judgement,
                excellent: judgement.excellent,
                note_score: judgement.note_score,
                missed: false,
            });
        }
        for _ in matched..*expected_count {
            summary.results.push(HitResult {
                expected_id: Some("failed-repeat-miss".into()),
                expected_beat: None,
                expected_timestamp_ms: None,
                observed_ms: None,
                actual_timestamp_ms: None,
                error_ms: None,
                window_ms: DEFAULT_MATCH_MS,
                instrument: Instrument::Unknown,
                midi_note: None,
                judgement: Judgement::Missed,
                excellent: false,
                note_score: 0.0,
                missed: true,
            });
        }
    }
    summary.score = (summary.expected > 0).then(|| {
        (((score_sum - 50.0 * invalid_hits as f64) / summary.expected as f64).clamp(0.0, 100.0)
            * 10.0)
            .round()
            / 10.0
    });
}

fn display_expected_id(id: &str) -> String {
    id.rsplit_once("-loop")
        .filter(|(_, repetition)| repetition.parse::<u8>().is_ok())
        .map(|(base, _)| base.to_string())
        .unwrap_or_else(|| id.to_string())
}

fn live_feedback_from_scoring(
    feedback: &[ScoringFeedback],
    sequence: &BarSequence,
    bpm: f64,
    tempo_map: &[TempoChange],
    origin_ms: f64,
    latency_ms: f64,
) -> Vec<LiveJudgement> {
    feedback
        .iter()
        .enumerate()
        .map(|(index, feedback)| {
            let result = &feedback.result;
            let expanded_id = result.expected_id.as_deref();
            let actual_timeline_beat = result.actual_timestamp_ms.map(|timestamp| {
                beat_at_elapsed_ms(timestamp - latency_ms - origin_ms, bpm, tempo_map)
            });
            LiveJudgement {
                expected_id: expanded_id.map(display_expected_id),
                iteration: expanded_id
                    .and_then(|id| id.rsplit_once("-loop"))
                    .and_then(|(_, suffix)| suffix.parse::<usize>().ok())
                    .and_then(|value| value.checked_sub(1))
                    .or_else(|| {
                        actual_timeline_beat.map(|beat| {
                            ((beat - sequence.origin).max(0.0) / sequence.length()).floor() as usize
                        })
                    }),
                expected_beat: result.expected_beat.map(|beat| sequence.display_beat(beat)),
                actual_beat: actual_timeline_beat
                    .map(|beat| sequence.display_beat(beat.max(sequence.origin))),
                instrument: result.instrument,
                judgement: result.judgement,
                error_ms: result.error_ms.unwrap_or(0.0),
                excellent: result.excellent,
                note_score: result.note_score,
                event_id: Some(format!(
                    "{}:{}:{}",
                    expanded_id.unwrap_or("unmatched"),
                    result
                        .actual_timestamp_ms
                        .map(|value| format!("{value:.3}"))
                        .unwrap_or_else(|| "none".into()),
                    index
                )),
                confirmed: feedback.confirmed,
            }
        })
        .collect()
}

fn merge_live_feedback(current: &mut Vec<LiveJudgement>, update: Vec<LiveJudgement>) {
    // Every update carries the complete current provisional projection. Keep
    // only its final value for each key, then replace matching confirmed
    // values in one pass. Repeated retain calls made a burst of provisional
    // invalid strikes quadratic before any renderer work began.
    let mut expected_keys = HashSet::new();
    let mut event_keys = HashSet::new();
    let mut replacement = update
        .into_iter()
        .rev()
        .filter(|judgement| {
            if let Some(expected_id) = &judgement.expected_id {
                expected_keys.insert((expected_id.clone(), judgement.iteration))
            } else if let Some(event_id) = &judgement.event_id {
                event_keys.insert(event_id.clone())
            } else {
                true
            }
        })
        .collect::<Vec<_>>();
    replacement.reverse();
    current.retain(|judgement| {
        judgement.confirmed
            && if let Some(expected_id) = &judgement.expected_id {
                !expected_keys.contains(&(expected_id.clone(), judgement.iteration))
            } else if let Some(event_id) = &judgement.event_id {
                !event_keys.contains(event_id)
            } else {
                true
            }
    });
    current.extend(replacement);
    // The scorer retains every confirmed invalid hit for the authoritative
    // summary. Only bound the visual projection: thousands of overlapping
    // unmatched markers are no longer readable and would make a long
    // continuous practice session progressively more expensive to render.
    let unmatched = current
        .iter()
        .filter(|judgement| judgement.expected_id.is_none())
        .count();
    let mut discard = unmatched.saturating_sub(LIVE_UNMATCHED_FEEDBACK_LIMIT);
    current.retain(|judgement| {
        if judgement.expected_id.is_none() && discard > 0 {
            discard -= 1;
            false
        } else {
            true
        }
    });
}

fn combined_summaries(summaries: &[SessionSummary]) -> SessionSummary {
    let expected = summaries.iter().map(|summary| summary.expected).sum();
    let results = summaries
        .iter()
        .flat_map(|summary| summary.results.iter().cloned())
        .collect();
    summary_from_results(expected, results)
}

fn confirmed_attempt_is_good(summary: &SessionSummary, tolerance: RepeatTolerance) -> bool {
    summary.expected > 0
        && summary.matched == summary.expected
        && summary.extras == 0
        && summary.wrong_instrument == 0
        && summary
            .results
            .iter()
            .filter(|result| result.expected_id.is_some())
            .all(|result| match tolerance {
                RepeatTolerance::Relaxed => matches!(
                    result.judgement,
                    Judgement::Perfect | Judgement::Good | Judgement::Early | Judgement::Late
                ),
                RepeatTolerance::Standard => {
                    matches!(result.judgement, Judgement::Perfect | Judgement::Good)
                }
                RepeatTolerance::Strict => result.judgement == Judgement::Perfect,
            })
}

#[cfg(test)]
struct LiveTimingContext<'a> {
    session_start_ms: f64,
    bpm: f64,
    tempo_map: &'a [TempoChange],
    sequence: &'a BarSequence,
    latency_ms: f64,
    config: ScoringConfig,
}

#[cfg(test)]
fn live_judgement_for_hit(
    expected: &[ExpectedHit],
    matched_expected_ids: &mut HashSet<String>,
    observed: &ObservedHit,
    context: &LiveTimingContext<'_>,
) -> LiveJudgement {
    let actual_beat = beat_at_elapsed_ms(
        observed.timestamp_ms - context.latency_ms - context.session_start_ms,
        context.bpm,
        context.tempo_map,
    );
    let match_ms = context
        .config
        .match_ms
        .max(context.config.good_ms)
        .max(context.config.perfect_ms);
    let timing_error = |hit: &ExpectedHit| {
        observed.timestamp_ms
            - context.latency_ms
            - (context.session_start_ms
                + elapsed_ms_at_beat(hit.beat, context.bpm, context.tempo_map))
    };
    let nearest = expected
        .iter()
        .filter(|hit| {
            hit.instrument == observed.instrument && !matched_expected_ids.contains(&hit.id)
        })
        .filter_map(|hit| {
            let error = timing_error(hit);
            (error.is_finite() && error.abs() <= match_ms).then_some((hit, error))
        })
        .min_by(|(left_hit, left_error), (right_hit, right_error)| {
            left_error
                .abs()
                .total_cmp(&right_error.abs())
                .then_with(|| left_hit.beat.total_cmp(&right_hit.beat))
        });

    if let Some((hit, error_ms)) = nearest {
        matched_expected_ids.insert(hit.id.clone());
        let judgement = context.config.classify(error_ms);
        return LiveJudgement {
            expected_id: Some(display_expected_id(&hit.id)),
            iteration: hit
                .id
                .rsplit_once("-loop")
                .and_then(|(_, suffix)| suffix.parse::<usize>().ok())
                .and_then(|value| value.checked_sub(1)),
            expected_beat: Some(context.sequence.display_beat(hit.beat)),
            actual_beat: Some(context.sequence.display_beat(actual_beat)),
            instrument: observed.instrument,
            judgement,
            error_ms,
            excellent: judgement == Judgement::Perfect,
            note_score: context.config.note_score(error_ms),
            event_id: Some(format!("{:.3}", observed.timestamp_ms)),
            confirmed: false,
        };
    }

    let wrong_instrument = expected.iter().any(|hit| {
        hit.instrument != observed.instrument
            && !matched_expected_ids.contains(&hit.id)
            && timing_error(hit).abs() <= match_ms
    });
    LiveJudgement {
        expected_id: None,
        iteration: None,
        expected_beat: None,
        actual_beat: Some(
            context
                .sequence
                .display_beat(actual_beat.max(context.sequence.origin)),
        ),
        instrument: observed.instrument,
        judgement: if wrong_instrument {
            Judgement::WrongInstrument
        } else {
            Judgement::Extra
        },
        error_ms: 0.0,
        excellent: false,
        note_score: 0.0,
        event_id: Some(format!("{:.3}", observed.timestamp_ms)),
        confirmed: false,
    }
}

fn persist_settings(settings: Settings) {
    if let Ok(value) = serde_wasm_bindgen::to_value(&settings) {
        spawn_local(async move {
            let _ = JsFuture::from(save_settings(value)).await;
        });
    }
}

fn restore_stats_focus() {
    if let Some(button) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("stats-trigger"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = button.focus();
    }
}

fn focus_stats_close() {
    let callback = Closure::once_into_js(|| {
        if let Some(button) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("stats-close"))
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = button.focus();
        }
    });
    if let Some(window) = web_sys::window() {
        let _ = window.request_animation_frame(callback.unchecked_ref());
    }
}

fn restore_library_focus() {
    if let Some(button) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("midi-library-trigger"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = button.focus();
    }
}

fn focus_library_close() {
    let callback = Closure::once_into_js(|| {
        if let Some(button) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("midi-library-close"))
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = button.focus();
        }
    });
    if let Some(window) = web_sys::window() {
        let _ = window.request_animation_frame(callback.unchecked_ref());
    }
}

fn focus_import_delete(hash: String) {
    let callback = Closure::once_into_js(move || {
        if let Some(button) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(&format!("delete-import-{hash}")))
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = button.focus();
        }
    });
    if let Some(window) = web_sys::window() {
        let _ = window.request_animation_frame(callback.unchecked_ref());
    }
}

fn focus_import_delete_cancel() {
    let callback = Closure::once_into_js(|| {
        if let Some(button) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("import-delete-cancel"))
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = button.focus();
        }
    });
    if let Some(window) = web_sys::window() {
        let _ = window.request_animation_frame(callback.unchecked_ref());
    }
}

fn dismiss_import_delete(pending_import_delete: RwSignal<Option<(String, String)>>, hash: String) {
    pending_import_delete.set(None);
    focus_import_delete(hash);
}

fn restore_settings_focus() {
    if let Some(button) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("settings-trigger"))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = button.focus();
    }
}

fn focus_settings_close() {
    let callback = Closure::once_into_js(|| {
        if let Some(button) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("settings-close"))
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = button.focus();
        }
    });
    if let Some(window) = web_sys::window() {
        let _ = window.request_animation_frame(callback.unchecked_ref());
    }
}

fn restore_result_focus() {
    if let Some(button) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.query_selector(".start-button").ok().flatten())
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = button.focus();
    }
}

fn focus_result_close() {
    let callback = Closure::once_into_js(|| {
        if let Some(button) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("result-modal-close"))
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = button.focus();
        }
    });
    if let Some(window) = web_sys::window() {
        let _ = window.request_animation_frame(callback.unchecked_ref());
    }
}

fn focus_midi_modal_close() {
    let callback = Closure::once_into_js(|| {
        if let Some(button) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id("midi-modal-close"))
            .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
        {
            let _ = button.focus();
        }
    });
    if let Some(window) = web_sys::window() {
        let _ = window.request_animation_frame(callback.unchecked_ref());
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ContextPopup {
    Bars,
    Metronome,
}

fn focus_popup_trigger(popup: ContextPopup) {
    let id = match popup {
        ContextPopup::Bars => "bar-selection-trigger",
        ContextPopup::Metronome => "visual-metronome",
    };
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id(id))
        .and_then(|element| element.dyn_into::<web_sys::HtmlElement>().ok())
    {
        let _ = element.focus();
    }
}

fn keyboard_targets_control(event: &web_sys::KeyboardEvent) -> bool {
    event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
        .and_then(|element| element.closest("button, summary, a[href]").ok().flatten())
        .is_some()
}

fn dismiss_result(
    result_modal_open: RwSignal<bool>,
    result_dismiss_after_ms: RwSignal<f64>,
    historical_result: RwSignal<Option<SessionRecord>>,
    stats_open: RwSignal<bool>,
) {
    result_modal_open.set(false);
    result_dismiss_after_ms.set(f64::INFINITY);
    if historical_result.get_untracked().is_some() {
        historical_result.set(None);
        stats_open.set(true);
        focus_stats_close();
    } else {
        clear_score_feedback();
        restore_result_focus();
    }
}

/// The one MIDI configuration surface, shared by the settings drawer and the
/// MIDI connection modal so settings cannot drift apart. It renders the
/// connection state, device selection, pad mapping and diagnostics, and can
/// retry the connection. Attempting a MIDI-dependent activity (pad mapping)
/// without a connected kit reopens the connection modal.
#[component]
fn MidiConfigSection(
    midi_status: RwSignal<String>,
    midi_ports: RwSignal<Vec<MidiPort>>,
    selected_midi: RwSignal<Option<String>>,
    midi_learn: RwSignal<Option<Instrument>>,
    midi_bindings: RwSignal<Vec<MidiBinding>>,
    diagnostics: RwSignal<Vec<String>>,
    midi_modal_open: RwSignal<bool>,
    connect: Rc<dyn Fn()>,
) -> impl IntoView {
    let connect_retry = connect.clone();
    view! {
      <div class="midi-config">
        <p class="midi-state">{move || midi_status.get()}</p>
        <button class="midi-retry" on:click=move |_|connect_retry()>"Retry connection"</button>
        <label class=move || if midi_ports.get().is_empty(){"hidden"}else{""}>"Input device"<select prop:value=move || selected_midi.get().unwrap_or_default() on:change=move |event|select_midi(&event_target_value(&event))><option value="" disabled=true hidden=true>"Choose a MIDI input"</option>{move || midi_ports.get().into_iter().map(|port|view!{<option value=port.id.clone()>{format!("{} · {}",port.manufacturer,port.name)}</option>}).collect_view()}</select></label>
        <div class="learn-card midi-mapping-card">
          <strong>{move || midi_learn.get().map(|instrument|format!("Hit your {} now…",instrument_label(instrument))).unwrap_or_else(||"Quick MIDI Learn".into())}</strong>
          <p class="help">"Map each pad independently. Unchanged General MIDI notes remain available as fallbacks."</p>
          <For
            each=move || vec![Instrument::Kick,Instrument::Snare,Instrument::ClosedHat,Instrument::OpenHat]
            key=|instrument|*instrument as u8
            children=move |instrument|{
              view!{
                <div class="midi-mapping-row">
                  <span>{move || midi_bindings.get().iter().find(|binding|binding.instrument==instrument).map(|binding|format!("{} · note {}, channel {}",instrument_label(instrument),binding.note,binding.channel)).unwrap_or_else(||format!("{} · General MIDI",instrument_label(instrument)))}</span>
                  <button aria-label=format!("Learn {}",instrument_label(instrument)) on:click=move |_|{
                    midi_learn.set(Some(instrument));
                    if !midi_connection_ready(&midi_ports.get_untracked(),selected_midi.get_untracked().as_deref()){
                      midi_modal_open.set(true);
                      focus_midi_modal_close();
                    }
                  }>{move || if midi_learn.get()==Some(instrument){"Listening…"}else{"Learn"}}</button>
                </div>
              }
            }
          />
        </div>
        <details><summary>"MIDI diagnostics"</summary><div class="diagnostics">{move || if diagnostics.get().is_empty(){view!{<p>"No MIDI messages received yet."</p>}.into_any()}else{diagnostics.get().into_iter().map(|line|view!{<code>{line}</code>}).collect_view().into_any()}}</div></details>
      </div>
    }
}

#[component]
fn MetronomeSettings(
    bpm: RwSignal<f64>,
    count_in: RwSignal<u8>,
    subdivision: RwSignal<u8>,
    timbre: RwSignal<String>,
    metronome: RwSignal<bool>,
    click_volume: RwSignal<f64>,
    snare_volume: RwSignal<f64>,
    snare_muted: RwSignal<bool>,
    error: RwSignal<Option<String>>,
    change_tempo: Rc<dyn Fn(f64)>,
    save: Rc<dyn Fn()>,
) -> impl IntoView {
    let tempo_input = change_tempo.clone();
    let tempo_input2 = change_tempo.clone();

    let tempo_commit = change_tempo.clone();
    let tempo_commit2 = change_tempo.clone();
    let tempo_enter = change_tempo;
    let save_input = save.clone();
    let save_input2 = save.clone();
    let save_commit = save.clone();
    let save_commit2 = save.clone();
    let save_enter = save.clone();
    let save_enabled = save.clone();
    let save_count = save.clone();
    let save_subdivision = save.clone();
    let save_timbre = save.clone();
    // The BPM value above the tempo slider must follow the thumb live. The
    // reactive `prop:value` binding already tracks `bpm`, but dragging was
    // observed leaving the number stale in the field (#18), so the slider's
    // own input/change events also mirror the authoritative tempo straight
    // into the display. Reading `bpm` after `change_tempo` keeps the mirror
    // desync-safe: when tempo changes are suppressed (adaptive sessions) the
    // unchanged tempo is mirrored instead of the raw slider position.
    let bpm_display = NodeRef::<leptos::html::Input>::new();
    let mirror_bpm_display = move || {
        if let Some(bpm_display) = bpm_display.get() {
            bpm_display.set_value(&bpm.get_untracked().round().to_string());
        }
    };
    let mirror_on_input = mirror_bpm_display;
    let mirror_on_commit = mirror_bpm_display;
    view! {
      <label class="toggle-row metronome-enabled"><span>"Click during exercise"</span><input type="checkbox" prop:checked=move || metronome.get() on:change=move |event|{metronome.set(event_target_checked(&event));save_enabled();}/></label>
      <div class="tempo-field"><span>"Tempo (BPM)"</span>
          <div class="tempo-value">
            <input aria-label="Tempo in BPM" title="Type a tempo from 30 to 240 BPM" type="number" inputmode="numeric" min="30" max="240" step="1"
              node_ref=bpm_display
              prop:value=move || bpm.get().round().to_string()
              on:focus=move |event|{event_target::<web_sys::HtmlInputElement>(&event).select();}
              on:input=move |event|{
                if let Ok(value)=event_target_value(&event).parse::<f64>() && (30.0..=240.0).contains(&value){
                  tempo_input2(value.round());save_input2();
                }
              }
              on:keydown=move |event: web_sys::KeyboardEvent|{
                if event.key()=="Enter" {
                  event.prevent_default();
                  event.stop_propagation();
                  let input=event_target::<web_sys::HtmlInputElement>(&event);
                  let value=input.value().parse::<f64>().ok().filter(|value|value.is_finite()).unwrap_or_else(||bpm.get_untracked()).round().clamp(30.0,240.0);
                  tempo_enter(value);input.set_value(&value.to_string());save_enter();
                }
              }
              on:change=move |event|{
                let input=event_target::<web_sys::HtmlInputElement>(&event);
                let value=input.value().parse::<f64>().ok().filter(|value|value.is_finite()).unwrap_or_else(||bpm.get_untracked()).round().clamp(30.0,240.0);
                tempo_commit2(value);input.set_value(&value.to_string());save_commit2();
              }/>
            <span>"BPM"</span>
          </div>
        <input type="range" class="tempo-slider" aria-label="Tempo slider" title="Drag to change tempo from 30 to 240 BPM" min="30" max="240" step="1"
          prop:value=move || bpm.get().round().to_string()
          on:input=move |event|{
            if let Ok(value)=event_target_value(&event).parse::<f64>() {
              tempo_input(value.round());save_input();
              mirror_on_input();
            }
          }
          on:change=move |event|{
            let input=event_target::<web_sys::HtmlInputElement>(&event);
            let value=input.value().parse::<f64>().ok().filter(|value|value.is_finite()).unwrap_or_else(||bpm.get_untracked()).round().clamp(30.0,240.0);
            tempo_commit(value);input.set_value(&value.to_string());mirror_on_commit();save_commit();
          }/>
      </div>
      <label class="metronome-count-in">"Count-in"<select prop:value=move || count_in.get().to_string() on:change=move |event|{count_in.set(event_target_value(&event).parse().unwrap_or(1));save_count();}><option value="0">"Off"</option><option value="1">"1 bar"</option><option value="2">"2 bars"</option></select></label>
      <label class="metronome-subdivision">"Subdivision"<select prop:value=move || subdivision.get().to_string() on:change=move |event|{subdivision.set(event_target_value(&event).parse().unwrap_or(1));save_subdivision();}><option value="1">"Quarter notes"</option><option value="2">"Eighth notes"</option><option value="4">"Sixteenth notes"</option></select></label>
      <label class="metronome-timbre">"Click timbre"<div class="inline-select"><select prop:value=move || timbre.get() on:change=move |event|{timbre.set(event_target_value(&event));save_timbre();}><option value="woodblock">"Warm woodblock"</option><option value="crisp">"Crisp click"</option><option value="cowbell">"Cowbell"</option></select><button type="button" on:click=move |_|{
        error.set(None);
        let promise=preview_click(&timbre.get_untracked());
        spawn_local(async move{
          if let Err(detail)=JsFuture::from(promise).await{
            let detail=detail.as_string().unwrap_or_else(||format!("{detail:?}"));
            error.set(Some(format!("Metronome preview failed: {detail}")));
          }
        });
      }>"Preview"</button></div></label>
      <small role="alert" class=move || if error.get().is_some(){"metronome-preview-error"}else{"metronome-preview-error hidden"}>{move || error.get().unwrap_or_default()}</small>
      <label class="metronome-volume">"Metronome volume"<input type="range" min="0" max="1" step="0.01" prop:value=move || click_volume.get() on:input=move |event|{let value=event_target_value(&event).parse().unwrap_or(0.72);click_volume.set(value);set_volumes(if snare_muted.get_untracked(){0.0}else{snare_volume.get_untracked()},value);save();}/></label>
      <small class="metronome-scheduling-help">"Count-in applies on the next start; subdivision, timbre and click enablement on the next start or resume."</small>
    }
}

#[component]
fn App() -> impl IntoView {
    let exercise = RwSignal::new(paradiddle());
    let imported_bytes = RwSignal::new(Vec::<u8>::new());
    let imported_name = RwSignal::new(String::new());
    let saved_imports = RwSignal::new(Vec::<SavedMidiImport>::new());
    let selected_import = RwSignal::new(None::<String>);
    let restoring_import = RwSignal::new(false);
    let phase = RwSignal::new("idle".to_string());
    let bpm = RwSignal::new(60.0);
    let count_in = RwSignal::new(1u8);
    let subdivision = RwSignal::new(1u8);
    let timbre = RwSignal::new("woodblock".to_string());
    let metronome = RwSignal::new(true);
    let click_volume = RwSignal::new(0.72);
    let snare_volume = RwSignal::new(0.72);
    let snare_muted = RwSignal::new(false);
    let snare_reference_mode = RwSignal::new("hits".to_string());
    let app_mode = RwSignal::new(AppMode::Practice);
    let highway = RwSignal::new(saved_highway_mode());
    let notation_dark = RwSignal::new(saved_notation_theme());
    let loops = RwSignal::new(1u8);
    let repeat_bar_until_good = RwSignal::new(false);
    let repeat_tolerance = RwSignal::new(RepeatTolerance::Standard);
    let selected_bars = RwSignal::new(Vec::<usize>::new());
    let pending_legacy_selection = RwSignal::new(None::<(String, usize, usize)>);
    let adaptive_enabled = RwSignal::new(false);
    let adaptive_settings = RwSignal::new(AdaptiveSettings::default());
    let adaptive_session_active = RwSignal::new(false);
    let adaptive_bars = RwSignal::new(None::<Vec<usize>>);
    let adaptive_status = RwSignal::new(None::<AdaptiveStatus>);
    let adaptive_original_bars = RwSignal::new(Vec::<usize>::new());
    let adaptive_original_bpm = RwSignal::new(60.0);
    let adaptive_original_loops = RwSignal::new(1u8);
    let adaptive_controller = Rc::new(RefCell::new(None::<AdaptiveController>));
    #[allow(clippy::type_complexity)]
    let adaptive_advance_holder: Rc<RefCell<Option<Rc<dyn Fn(SessionSummary)>>>> =
        Rc::new(RefCell::new(None));
    let bar_sequence = Memo::new(move |_| {
        let bars = adaptive_bars.get().unwrap_or_else(|| selected_bars.get());
        Arc::new(BarSequence::new(&exercise.get(), &bars))
    });
    let selection_changed = RwSignal::new(false);
    let context_popup = RwSignal::new(None::<ContextPopup>);
    let progress = RwSignal::new(-1.0);
    // Per-frame `progress` updates re-ran the bar label and timeline views on
    // every transport frame even when the displayed bar had not changed.
    // `displayed_bar` memoises the label's one-based bar index (its closure
    // re-evaluates per frame but only notifies the view when the bar changes);
    // `timeline_beat` samples the same beat at ~10 Hz for the thin progress
    // bar. Scoring, pause/resume and the transport keep using `progress`.
    let displayed_bar = Memo::new(move |_| {
        let sequence = bar_sequence.get();
        let beat: f64 = progress.get();
        (sequence.display_beat(beat.max(sequence.origin)) / sequence.bar_beats).floor() as usize + 1
    });
    let timeline_beat = RwSignal::new(-1.0);
    let run_start_ms = RwSignal::new(0.0);
    let paused_run_start_ms = RwSignal::new(None::<f64>);
    let run_expected = RwSignal::new(Arc::new(Vec::<ExpectedHit>::new()));
    let active_segment_start = RwSignal::new(0.0);
    let visible_loop_index = RwSignal::new(0usize);
    let observed = RwSignal::new(Vec::<ObservedHit>::new());
    let feedback_dirty = RwSignal::new(true);
    let accepted_observed = RwSignal::new(Vec::<ObservedHit>::new());
    let practice_origin_ms = RwSignal::new(0.0);
    let bar_attempt = RwSignal::new(1usize);
    let failed_repeat_attempts = RwSignal::new(Vec::<(usize, Vec<LiveJudgement>)>::new());
    let repeat_attempt_summaries = RwSignal::new(Vec::<SessionSummary>::new());
    let live = RwSignal::new(Vec::<LiveJudgement>::new());
    let live_matched_expected = RwSignal::new(HashSet::<String>::new());
    let input_mode = RwSignal::new(InputSource::Demo);
    let result = RwSignal::new(None::<SessionRecord>);
    let historical_result = RwSignal::new(None::<SessionRecord>);
    let displayed_result = Signal::derive(move || historical_result.get().or_else(|| result.get()));
    let history = RwSignal::new(Vec::<SessionRecord>::new());
    let incomplete_history = RwSignal::new(0usize);
    let practice_time_ms = RwSignal::new(None::<f64>);
    let comparison = RwSignal::new(None::<f64>);
    let settings_open = RwSignal::new(false);
    let stats_open = RwSignal::new(false);
    let library_open = RwSignal::new(false);
    let pending_import_delete = RwSignal::new(None::<(String, String)>);
    let pending_clear_history = RwSignal::new(false);
    let pending_reset_settings = RwSignal::new(false);
    let library_personal = RwSignal::new(false);
    let library_page = RwSignal::new(0usize);
    let library_difficulty = RwSignal::new(None::<Difficulty>);
    // Playhead-smoothness diagnostics: session-only switches seeded from the
    // bridge so URL parameters still work, while the Settings drawer toggles
    // them at runtime without URL editing. They are never persisted.
    let diagnostics_state: DiagnosticsSnapshot =
        serde_wasm_bindgen::from_value(diagnostics_snapshot()).unwrap_or_default();
    let diagnostic_reference_overlay = RwSignal::new(diagnostics_state.reference_overlay);
    let diagnostic_visual_feedback = RwSignal::new(diagnostics_state.visual_feedback);
    let diagnostic_metronome_visual = RwSignal::new(diagnostics_state.metronome_visual);
    let diagnostic_metronome_needle = RwSignal::new(diagnostics_state.metronome_needle);
    let diagnostic_metronome_pulses = RwSignal::new(diagnostics_state.metronome_pulses);
    let diagnostic_state_tick = RwSignal::new(diagnostics_state.state_tick);
    let diagnostic_auto_scroll = RwSignal::new(diagnostics_state.auto_scroll);
    let diagnostic_perf_counters = RwSignal::new(diagnostics_state.perf_counters);
    // A run whose scoring/UI tick was disabled — from its start or part-way —
    // is a diagnostic run: its summary is not a valid performance and must
    // not be saved or compared.
    let diagnostic_run_invalid = RwSignal::new(false);
    let result_modal_open = RwSignal::new(false);
    let result_dismiss_after_ms = RwSignal::new(f64::INFINITY);
    let auto_show_statistics = RwSignal::new(true);
    let welcome = RwSignal::new(true);
    // Dedicated first-run onboarding page. It is open from the first paint
    // (seeded synchronously from local preference storage), so returning users
    // who opted out never see it and fresh users never see the workspace
    // flash first. The skip choice only persists when "Don't show me this
    // again" is checked; otherwise the dismissal is session-only.
    let onboarding_open = RwSignal::new(!saved_onboarding_skip());
    let onboarding_skip = RwSignal::new(saved_onboarding_skip());
    let import_error = RwSignal::new(None::<String>);
    let metronome_preview_error = RwSignal::new(None::<String>);
    let midi_status = RwSignal::new("Checking for MIDI drums…".to_string());
    let midi_ports = RwSignal::new(Vec::<MidiPort>::new());
    let selected_midi = RwSignal::new(None::<String>);
    let diagnostics = RwSignal::new(Vec::<String>::new());
    let midi_learn = RwSignal::new(None::<Instrument>);
    let midi_bindings = RwSignal::new(default_midi_bindings());
    let midi_profiles = RwSignal::new(Vec::<SavedMidiProfile>::new());
    // Manual MIDI connection modal. It only opens on demand: from the settings
    // drawer's "MIDI connection setup…" control or when a MIDI-dependent
    // activity (pad mapping) needs a connected kit. The automatic three-second
    // startup modal is retired; first-run setup now lives on the onboarding
    // page.
    let midi_modal_open = RwSignal::new(false);
    let midi_permission = RwSignal::new(None::<String>);
    let settings_loaded = RwSignal::new(false);
    let snare_note = RwSignal::new(38u8);
    let snare_channel = RwSignal::new(10u8);
    let latency_ms = RwSignal::new(0.0);
    let calibration_targets = RwSignal::new(Vec::<f64>::new());
    let calibration_errors = RwSignal::new(Vec::<f64>::new());
    let on_time_ms = RwSignal::new(DEFAULT_GOOD_MS);
    let excellent_ms = RwSignal::new(DEFAULT_PERFECT_MS);
    let runtime_tempo_map = RwSignal::new(scaled_tempo_map(&paradiddle(), 1, 60.0));
    let scoring_engine = Rc::new(RefCell::new(None::<IncrementalScorer>));

    // The optional presentation renderer reads the same state as notation.
    // Its position is supplied by the existing Tone frame in the bridge, not
    // by a reactive per-frame effect or an additional transport subscription.
    let presentation_config = move || {
        let sequence = bar_sequence.get_untracked();
        let repetitions = if loops.get_untracked() == 0 {
            CONTINUOUS_LOOP_CHUNK
        } else {
            loops.get_untracked()
        };
        let position = progress.get_untracked();
        let current_phase = phase.get_untracked();
        // Before a new run, show the same count-in position that Tone will
        // report on its first frame, rather than jumping there on Start.
        let presentation_position =
            if current_phase == "idle" || (current_phase == "count_in" && position < 0.0) {
                sequence.origin - sequence.bar_beats * f64::from(count_in.get_untracked())
            } else if position < 0.0 && current_phase != "count_in" {
                sequence.origin
            } else {
                position
            };
        serde_json::json!({
            "bars": sequence.bars,
            "endBeat": sequence.origin + sequence.length() * f64::from(repetitions),
            "position": presentation_position,
            "phase": current_phase,
            "tempoBpm": bpm.get_untracked(),
            "loops": loops.get_untracked(),
            "repeatUntilGood": repeat_bar_until_good.get_untracked(),
            "barAttempt": bar_attempt.get_untracked(),
        })
    };
    let snapshot = Closure::wrap(Box::new(move || -> JsValue {
        json_compatible(&serde_json::json!({
            "exercise": presentation_exercise(&exercise.get_untracked()),
            "config": presentation_config(),
            "judgements": live.get_untracked(),
        }))
        .unwrap_or(JsValue::NULL)
    }) as Box<dyn FnMut() -> JsValue>);
    bind_presentation_snapshot(snapshot.as_ref().unchecked_ref());
    let _snapshot = StoredValue::new_local(snapshot);
    on_cleanup(unbind_presentation);
    Effect::new(move |_| set_highway_mode(highway.get()));
    Effect::new(move |_| {
        exercise.track();
        if presentation_active() || highway.get_untracked() {
            presentation_exercise_changed();
        }
    });
    Effect::new(move |_| {
        selected_bars.track();
        exercise.track();
        loops.track();
        repeat_bar_until_good.track();
        bar_attempt.track();
        phase.track();
        bpm.track();
        count_in.track();
        if presentation_active()
            && let Ok(value) = json_compatible(&presentation_config())
        {
            presentation_configuration(value);
        }
    });
    Effect::new(move |_| {
        live.track();
        if presentation_active()
            && let Ok(value) = json_compatible(&live.get_untracked())
        {
            presentation_judgements(value);
        }
    });
    Effect::new(move |_| {
        if !settings_loaded.get() {
            return;
        }
        let Some(id) = selected_midi.get() else {
            return;
        };
        if let Some(profile) = midi_profiles
            .get()
            .into_iter()
            .find(|profile| profile.id == id && !profile.bindings.is_empty())
        {
            midi_bindings.set(profile.bindings);
        }
    });

    let snapshot_settings: Rc<dyn Fn() -> Settings> = Rc::new(move || Settings {
        mode: Some(app_mode.get()),
        bpm: Some(bpm.get()),
        count_in_bars: Some(count_in.get()),
        subdivision: Some(subdivision.get()),
        timbre: Some(timbre.get()),
        metronome: Some(metronome.get()),
        click_volume: Some(click_volume.get()),
        snare_volume: Some(snare_volume.get()),
        snare_muted: Some(snare_muted.get()),
        snare_reference_mode: Some(snare_reference_mode.get()),
        loops: Some(loops.get()),
        repeat_bar_until_good: Some(repeat_bar_until_good.get()),
        repeat_tolerance: Some(repeat_tolerance.get()),
        selection_start_bar: None,
        selection_end_bar: None,
        selected_bars: Some(selected_bars.get()),
        latency_ms: Some(latency_ms.get()),
        on_time_ms: Some(on_time_ms.get()),
        excellent_ms: Some(excellent_ms.get()),
        snare_note: Some(snare_note.get()),
        snare_channel: Some(snare_channel.get()),
        midi_bindings: Some(midi_bindings.get()),
        welcome_dismissed: Some(!welcome.get()),
        last_import_hash: selected_import.get(),
        auto_show_statistics: Some(auto_show_statistics.get()),
        adaptive_enabled: Some(adaptive_enabled.get()),
        adaptive: Some(adaptive_settings.get()),
        library_difficulty: library_difficulty
            .get()
            .map(|difficulty| difficulty.label().to_string()),
        selected_track: Some(exercise.get().selected_track),
    });

    let save_current_settings: Rc<dyn Fn()> = {
        let snapshot = snapshot_settings.clone();
        Rc::new(move || persist_settings(snapshot()))
    };
    ADAPTIVE_SAVE.with(|cell| {
        *cell.borrow_mut() = Some(save_current_settings.clone());
    });

    // Safety net for persistence: re-reads every persisted setting, so any
    // change - from a control without its own save trigger or from programmatic
    // updates such as MIDI profiles, adaptive sessions or imports - reaches the
    // stored settings. Interactive controls keep their immediate save triggers;
    // this debounce coalesces everything else, and `pagehide` flushes pending
    // state so closing the browser also retains the latest settings.
    let auto_save_timer: Rc<RefCell<Option<i32>>> = Rc::new(RefCell::new(None));
    Effect::new(move |_| {
        if !settings_loaded.get() {
            return;
        }
        let snapshot = snapshot_settings();
        let Some(window) = web_sys::window() else {
            return;
        };
        if let Some(handle) = auto_save_timer.borrow_mut().take() {
            window.clear_timeout_with_handle(handle);
        }
        let commit = Closure::once_into_js(move || persist_settings(snapshot));
        if let Ok(handle) = window
            .set_timeout_with_callback_and_timeout_and_arguments_0(commit.unchecked_ref(), 300)
        {
            *auto_save_timer.borrow_mut() = Some(handle);
        }
    });
    {
        let flush = save_current_settings.clone();
        let listener = Closure::wrap(Box::new(move || {
            flush();
        }) as Box<dyn Fn()>);
        if let Some(window) = web_sys::window() {
            let _ = window
                .add_event_listener_with_callback("pagehide", listener.as_ref().unchecked_ref());
        }
        listener.forget();
    }

    let scoring_for_finish = scoring_engine.clone();
    let finish_run: Rc<dyn Fn()> = Rc::new(move || {
        set_practice_active(false);
        phase.set("finishing".into());
        // Diagnostic runs (scoring/UI tick disabled at any point) are not
        // performances: skip the scorer — which may never have been created —
        // never save or compare, and return to idle without an error.
        if diagnostic_run_invalid.get_untracked() || !diagnostic_state_tick.get_untracked() {
            scoring_for_finish.borrow_mut().take();
            result.set(None);
            comparison.set(None);
            phase.set("complete".into());
            return;
        }
        let attempts = repeat_attempt_summaries.get_untracked();
        let summary = if attempts.is_empty() {
            let mut scoring = scoring_for_finish.borrow_mut();
            match scoring.as_mut().map(IncrementalScorer::finish) {
                Some(Ok(summary)) => summary,
                Some(Err(error)) => {
                    import_error.set(Some(format!("Scoring could not finish safely: {error}")));
                    phase.set("error".into());
                    return;
                }
                None => {
                    import_error.set(Some("Scoring state was unavailable at completion.".into()));
                    phase.set("error".into());
                    return;
                }
            }
        } else {
            combined_summaries(&attempts)
        };
        let current = SessionRecord {
            id: format!("{}-{:.0}", exercise.get_untracked().id, Date::now()),
            ended_at: Date::new_0().to_iso_string().into(),
            exercise_id: exercise.get_untracked().id.clone(),
            exercise_title: exercise.get_untracked().title.clone(),
            exercise_hash: exercise.get_untracked().content_hash.clone(),
            bpm: bpm.get_untracked(),
            input_mode: input_mode.get_untracked(),
            scoring_version: SCORING_VERSION.into(),
            tolerance_ms: on_time_ms.get_untracked(),
            latency_ms: latency_ms.get_untracked(),
            summary,
            observed: observed.get_untracked(),
            schema_version: 1,
            mode: app_mode.get_untracked(),
        };
        if app_mode.get_untracked() == AppMode::Practice {
            result.set(None);
            phase.set("complete".into());
            return;
        }
        let previous = history
            .get_untracked()
            .iter()
            .filter(|session| {
                session.exercise_hash == current.exercise_hash
                    && (session.bpm - current.bpm).abs() < 0.01
                    && session.input_mode == current.input_mode
                    && session.scoring_version == current.scoring_version
                    && (session.tolerance_ms - current.tolerance_ms).abs() < 0.01
            })
            .max_by(|a, b| a.ended_at.cmp(&b.ended_at))
            .cloned();
        comparison
            .set(previous.and_then(|item| Some(current.summary.score? - item.summary.score?)));
        if let Ok(value) = serde_wasm_bindgen::to_value(&current) {
            spawn_local(async move {
                let _ = JsFuture::from(save_session(value)).await;
            });
        }
        history.update(|items| items.push(current.clone()));
        result.set(Some(current));
        phase.set("complete".into());
        if auto_show_statistics.get_untracked() {
            result_dismiss_after_ms.set(
                web_sys::window()
                    .and_then(|window| window.performance())
                    .map(|performance| performance.now() + RESULT_DISMISS_GRACE_MS)
                    .unwrap_or(Date::now() + RESULT_DISMISS_GRACE_MS),
            );
            result_modal_open.set(true);
            focus_result_close();
        } else {
            result_dismiss_after_ms.set(f64::INFINITY);
        }
    });

    let frame_holder: CallbackSlot<FrameCallback> = Rc::new(RefCell::new(None));
    let done_holder: CallbackSlot<DoneCallback> = Rc::new(RefCell::new(None));
    let launch_holder: LaunchSlot = Rc::new(RefCell::new(None));
    let adaptive_advance_holder_for_launch = adaptive_advance_holder.clone();
    let launch_run: LaunchRun = {
        let finish_run = finish_run.clone();
        let frame_holder = frame_holder.clone();
        let done_holder = done_holder.clone();
        let launch_holder = launch_holder.clone();
        let scoring_for_launch = scoring_engine.clone();
        Rc::new(move |offset, count_beats, immediate| {
            phase.set(if count_beats > 0.0 {
                "count_in".into()
            } else {
                "starting".into()
            });
            // Each new run starts diagnostically clean; the flag is set when
            // the scoring/UI tick is disabled while a run is active.
            diagnostic_run_invalid.set(false);
            progress.set(offset - count_beats);
            timeline_beat.set(offset - count_beats);
            let current_exercise = exercise.get_untracked();
            let bar_beats = beats_per_bar(&current_exercise);
            let exercise_length = exercise_beats(&current_exercise);
            let sequence = bar_sequence.get_untracked();
            let range_start = sequence.origin;
            let configured_loops = if loops.get_untracked() == 0 {
                CONTINUOUS_LOOP_CHUNK
            } else {
                loops.get_untracked()
            };
            let total = range_start + sequence.length() * configured_loops as f64;
            let repeat_enabled = repeat_bar_until_good.get_untracked();
            let (segment_start, segment_end) = if repeat_enabled {
                repeat_bar_segment(offset, bar_beats, total)
            } else {
                (offset, total)
            };
            let scoring_expected = if repeat_enabled {
                hits_in_bar(
                    run_expected.get_untracked().as_ref(),
                    segment_start,
                    segment_end,
                )
            } else {
                run_expected.get_untracked().as_ref().clone()
            };
            let resuming = paused_run_start_ms.get_untracked().is_some();
            if !resuming {
                scoring_for_launch.borrow_mut().take();
            }
            active_segment_start.set(segment_start);
            let tempo_map =
                expanded_sequence_tempo_map(&current_exercise, &sequence, configured_loops);
            if !immediate {
                let scale = bpm.get_untracked() / current_exercise.tempo_bpm.max(1.0);
                runtime_tempo_map.set(
                    tempo_map
                        .iter()
                        .cloned()
                        .map(|mut change| {
                            change.bpm *= scale;
                            change
                        })
                        .collect(),
                );
            }
            let tempo_events = tempo_map
                .iter()
                .map(|change| serde_json::json!({"beat":change.beat,"bpm":change.bpm}))
                .collect::<Vec<_>>();
            let reference_notes = if !snare_muted.get_untracked()
                && snare_reference_mode.get_untracked() == "notes"
            {
                reference_drum_notes(run_expected.get_untracked().as_ref(), offset, segment_end)
            } else {
                Vec::new()
            };
            let final_source_bpm = tempo_map
                .iter()
                .take_while(|change| change.beat <= segment_end + 0.000_001)
                .last()
                .map(|change| change.bpm)
                .unwrap_or(current_exercise.tempo_bpm);
            let final_effective_bpm =
                final_source_bpm * bpm.get_untracked() / current_exercise.tempo_bpm.max(1.0);
            let scoring_tail_beats = (SCORING_TAIL_MS + latency_ms.get_untracked().max(0.0))
                * final_effective_bpm
                / 60_000.0;
            let options=json_compatible(&serde_json::json!({"bpm":bpm.get_untracked(),"sourceBpm":current_exercise.tempo_bpm,"tempoMap":tempo_events,"countInBeats":count_beats,"meter":bar_beats,"exerciseBeats":exercise_length,"displayRangeStart":range_start,"displayRangeBeats":sequence.length(),"displayBars":sequence.bars,"totalBeats":segment_end,"offsetBeat":offset,"immediate":immediate,"subdivision":subdivision.get_untracked(),"metronome":metronome.get_untracked(),"timbre":timbre.get_untracked(),"referenceNotes":reference_notes,"scoringTailBeats":scoring_tail_beats})).unwrap_or(JsValue::NULL);
            let origin_set = Rc::new(Cell::new(false));
            let tracking_started = Rc::new(Cell::new(false));
            let range_length = sequence.length();
            let initial_visible_loop =
                ((offset - range_start).max(0.0) / range_length).floor() as usize;
            visible_loop_index.set(initial_visible_loop);
            let scoring_for_frame = scoring_for_launch.clone();
            let frame_sequence = sequence.clone();
            let last_scoring_revision = Cell::new(u64::MAX);
            let timeline_last_ms = Cell::new(f64::NEG_INFINITY);
            let frame = Closure::wrap(Box::new(move |beat: f64, now: f64, start: f64| {
                let score_tempo_map = runtime_tempo_map.get_untracked();
                let scoring_bpm = score_tempo_map
                    .first()
                    .map(|change| change.bpm)
                    .unwrap_or(bpm.get_untracked());
                let session_start =
                    session_start_ms_at_beat(start, offset, scoring_bpm, &score_tempo_map);
                if let Some(previous_start) = paused_run_start_ms.get_untracked() {
                    observed
                        .update(|hits| rebase_hits_for_resume(hits, previous_start, session_start));
                    if let Some(scorer) = scoring_for_frame.borrow_mut().as_mut() {
                        scorer.rebase_origin(session_start);
                    }
                    paused_run_start_ms.set(None);
                }
                if scoring_for_frame.borrow().is_none() {
                    *scoring_for_frame.borrow_mut() = Some(IncrementalScorer::new(
                        &scoring_expected,
                        scoring_bpm,
                        session_start,
                        ScoringConfig {
                            perfect_ms: excellent_ms.get_untracked(),
                            good_ms: on_time_ms.get_untracked(),
                            ..ScoringConfig::default()
                        },
                        &score_tempo_map,
                    ));
                }
                // The run origin is constant during a run: skip the signal
                // write when unchanged so the 30 Hz tick does not churn
                // reactivity on devices with a tight frame budget.
                if run_start_ms.get_untracked() != session_start {
                    run_start_ms.set(session_start);
                }
                let visible_beat = beat.min(segment_end - 0.000_001);
                progress.set(visible_beat);
                // ~10 Hz is plenty for the 3 px timeline; the final frame of a
                // run still lands the bar exactly at its end position.
                if beat >= segment_end - 0.000_001 || now - timeline_last_ms.get() >= 100.0 {
                    timeline_last_ms.set(now);
                    timeline_beat.set(visible_beat);
                }
                if repeat_enabled
                    && !origin_set.replace(true)
                    && accepted_observed.get_untracked().is_empty()
                {
                    practice_origin_ms.set(session_start);
                }
                if beat >= offset - 0.000_001
                    && matches!(phase.get_untracked().as_str(), "count_in" | "starting")
                {
                    phase.set("running".into());
                }
                if beat >= offset - 0.000_001 && !tracking_started.replace(true) {
                    set_practice_active(true);
                }
                if !repeat_enabled && beat >= range_start {
                    let current_visible_loop =
                        ((beat - range_start) / range_length).floor() as usize;
                    if current_visible_loop > visible_loop_index.get_untracked()
                        && beat < total - 0.000_001
                    {
                        visible_loop_index.set(current_visible_loop);
                        // The scorer continues across iterations; only reset
                        // the renderer projection so a previous loop cannot
                        // leave stale notation colours or unbounded markers.
                        live.set(vec![]);
                        feedback_dirty.set(false);
                        clear_score_feedback_preserve_viewport();
                    }
                }
                if beat >= offset - 0.000_001 {
                    let watermark = (session_start
                        + elapsed_ms_at_beat(beat, scoring_bpm, &score_tempo_map))
                    .min(now)
                        - latency_ms.get_untracked().max(0.0)
                        - SCORING_DELIVERY_GRACE_MS;
                    let scoring_result = {
                        let mut scoring = scoring_for_frame.borrow_mut();
                        let scorer = scoring.as_mut().expect("scorer initialized above");
                        scorer.advance_event_time(watermark).and_then(|_| {
                            let revision = scorer.revision();
                            if revision == last_scoring_revision.get() {
                                Ok(None)
                            } else {
                                last_scoring_revision.set(revision);
                                scorer.take_feedback_update().map(Some)
                            }
                        })
                    };
                    match scoring_result {
                        Ok(Some(feedback)) => {
                            let mut update = live_feedback_from_scoring(
                                &feedback,
                                &frame_sequence,
                                scoring_bpm,
                                &score_tempo_map,
                                session_start,
                                latency_ms.get_untracked(),
                            );
                            if !repeat_enabled {
                                let iteration = visible_loop_index.get_untracked();
                                update.retain(|judgement| {
                                    judgement.iteration.is_none_or(|value| value >= iteration)
                                });
                            }
                            let mut visible = live.get_untracked();
                            merge_live_feedback(&mut visible, update);
                            if !repeat_enabled {
                                let iteration = visible_loop_index.get_untracked();
                                visible.retain(|judgement| {
                                    judgement.iteration.is_none_or(|value| value == iteration)
                                });
                            }
                            if live.get_untracked() != visible {
                                live.set(visible);
                                feedback_dirty.set(true);
                            }
                        }
                        Ok(None) => {}
                        Err(error) => {
                            set_practice_active(false);
                            phase.set("error".into());
                            import_error.set(Some(format!(
                                "Scoring stopped to preserve reliable results: {error}"
                            )));
                            stop_audio();
                            return;
                        }
                    }
                }
                if beat >= offset - 0.000_001 && feedback_dirty.get_untracked() {
                    feedback_dirty.set(false);
                    let visible = live.get_untracked();
                    let judgement_values =
                        serde_wasm_bindgen::to_value(&visible).unwrap_or(JsValue::NULL);
                    // Feedback only: the deferred scoring tick runs after the
                    // frame's commit, so it must not reposition the playhead
                    // (the transport frame owns position and scrolling, and a
                    // post-commit beat would rewind it visibly). Same pattern
                    // as the MIDI input turn.
                    queue_score_feedback(judgement_values);
                }
            }) as Box<dyn FnMut(f64, f64, f64)>);
            let done_finish = finish_run.clone();
            let repeat_launch = launch_holder.clone();
            let scoring_for_done = scoring_for_launch.clone();
            let value = adaptive_advance_holder_for_launch.clone();
            let done = Closure::wrap(Box::new(move || {
                set_practice_active(false);
                if adaptive_session_active.get_untracked() {
                    let attempt_summary = {
                        let mut scoring = scoring_for_done.borrow_mut();
                        match scoring.as_mut().map(IncrementalScorer::finish) {
                            Some(Ok(summary)) => summary,
                            Some(Err(error)) => {
                                phase.set("error".into());
                                import_error.set(Some(format!(
                                    "Scoring could not finish the adaptive attempt safely: {error}"
                                )));
                                return;
                            }
                            None => {
                                phase.set("error".into());
                                import_error.set(Some(
                                    "Scoring state was unavailable for the adaptive attempt."
                                        .into(),
                                ));
                                return;
                            }
                        }
                    };
                    observed.set(vec![]);
                    live.set(vec![]);
                    live_matched_expected.set(HashSet::new());
                    feedback_dirty.set(true);
                    clear_score_feedback_preserve_viewport();
                    scoring_for_done.borrow_mut().take();
                    let advance = value.borrow().as_ref().cloned();
                    if let Some(advance) = advance {
                        advance(attempt_summary);
                    }
                    return;
                }
                if !repeat_enabled {
                    if app_mode.get_untracked() == AppMode::Practice && loops.get_untracked() == 0 {
                        observed.set(vec![]);
                        live.set(vec![]);
                        live_matched_expected.set(HashSet::new());
                        scoring_for_done.borrow_mut().take();
                        feedback_dirty.set(true);
                        clear_score_feedback_preserve_viewport();
                        let next = repeat_launch.borrow().as_ref().cloned();
                        if let Some(launch) = next {
                            spawn_local(async move { launch(range_start, 0.0, true) });
                        }
                        return;
                    }
                    done_finish();
                    return;
                }
                let attempt_summary = {
                    let mut scoring = scoring_for_done.borrow_mut();
                    match scoring.as_mut().map(IncrementalScorer::finish) {
                        Some(Ok(summary)) => summary,
                        Some(Err(error)) => {
                            phase.set("error".into());
                            import_error.set(Some(format!(
                                "Scoring could not finish the practice attempt safely: {error}"
                            )));
                            return;
                        }
                        None => {
                            phase.set("error".into());
                            import_error.set(Some(
                                "Scoring state was unavailable for the practice attempt.".into(),
                            ));
                            return;
                        }
                    }
                };
                repeat_attempt_summaries.update(|items| items.push(attempt_summary.clone()));
                let attempt_origin = run_start_ms.get_untracked();
                if confirmed_attempt_is_good(&attempt_summary, repeat_tolerance.get_untracked()) {
                    let canonical_origin = practice_origin_ms.get_untracked();
                    let accepted = observed
                        .get_untracked()
                        .into_iter()
                        .map(|mut hit| {
                            hit.timestamp_ms =
                                canonical_origin + (hit.timestamp_ms - attempt_origin);
                            hit
                        })
                        .collect::<Vec<_>>();
                    accepted_observed.update(|items| items.extend(accepted));
                    observed.set(vec![]);
                    feedback_dirty.set(true);
                    live_matched_expected.set(HashSet::new());
                    scoring_for_done.borrow_mut().take();
                    bar_attempt.set(1);
                    if segment_end >= total - 0.000_001 {
                        observed.set(accepted_observed.get_untracked());
                        run_start_ms.set(canonical_origin);
                        done_finish();
                    } else {
                        live.set(vec![]);
                        clear_score_feedback_preserve_viewport();
                        let next = repeat_launch.borrow().as_ref().cloned();
                        if let Some(launch) = next {
                            spawn_local(async move { launch(segment_end, 0.0, true) });
                        }
                    }
                } else {
                    observed.set(vec![]);
                    feedback_dirty.set(true);
                    live.set(vec![]);
                    live_matched_expected.set(HashSet::new());
                    scoring_for_done.borrow_mut().take();
                    bar_attempt.update(|attempt| *attempt += 1);
                    clear_score_feedback_preserve_viewport();
                    let next = repeat_launch.borrow().as_ref().cloned();
                    if let Some(launch) = next {
                        spawn_local(async move { launch(segment_start, 0.0, true) });
                    }
                }
            }) as Box<dyn FnMut()>);
            let promise = start_audio(
                options,
                frame.as_ref().unchecked_ref(),
                done.as_ref().unchecked_ref(),
            );
            *frame_holder.borrow_mut() = Some(frame);
            *done_holder.borrow_mut() = Some(done);
            spawn_local(async move {
                match JsFuture::from(promise).await {
                    Ok(_) => {}
                    Err(error) => {
                        set_practice_active(false);
                        phase.set("error".into());
                        let detail = error.as_string().unwrap_or_else(|| format!("{error:?}"));
                        import_error.set(Some(format!("Audio failed: {detail}")));
                    }
                }
            });
        })
    };
    *launch_holder.borrow_mut() = Some(launch_run.clone());

    let pause_run: Rc<dyn Fn()> = Rc::new(move || {
        set_practice_active(false);
        let current_phase = phase.get_untracked();
        let audio_position = current_audio_beat();
        let position = if current_phase == "starting" || !audio_position.is_finite() {
            progress.get_untracked()
        } else {
            audio_position
        };
        progress.set(position);
        timeline_beat.set(position);
        if run_start_ms.get_untracked() > 0.0 {
            paused_run_start_ms.set(Some(run_start_ms.get_untracked()));
        }
        stop_audio();
        flush_score_feedback();
        phase.set("paused".into());
    });
    let start_holder: StartSlot = Rc::new(RefCell::new(None));
    let resume_run: Rc<dyn Fn()> = {
        let launch = launch_run.clone();
        let start_holder = start_holder.clone();
        Rc::new(move || {
            if phase.get_untracked() == "paused" {
                if selection_changed.get_untracked() {
                    if let Some(start) = start_holder.borrow().as_ref() {
                        start(count_in.get_untracked());
                    }
                } else {
                    let position = progress.get_untracked();
                    let segment_start = active_segment_start.get_untracked();
                    if position < segment_start {
                        launch(segment_start, segment_start - position, true);
                    } else {
                        launch(position.max(0.0), 0.0, true);
                    }
                }
            }
        })
    };

    let scoring_for_reset = scoring_engine.clone();
    let reset_run: Rc<dyn Fn()> = Rc::new(move || {
        set_practice_active(false);
        selection_changed.set(false);
        stop_audio();
        result_modal_open.set(false);
        observed.set(vec![]);
        feedback_dirty.set(true);
        accepted_observed.set(vec![]);
        practice_origin_ms.set(0.0);
        paused_run_start_ms.set(None);
        bar_attempt.set(1);
        failed_repeat_attempts.set(vec![]);
        repeat_attempt_summaries.set(vec![]);
        scoring_for_reset.borrow_mut().take();
        live.set(vec![]);
        live_matched_expected.set(HashSet::new());
        result.set(None);
        progress.set(-1.0);
        timeline_beat.set(-1.0);
        clear_score_feedback();
        phase.set("idle".into());
        if adaptive_session_active.get_untracked() {
            adaptive_session_active.set(false);
            adaptive_bars.set(None);
            adaptive_status.set(None);

            bpm.set(adaptive_original_bpm.get_untracked());
            loops.set(adaptive_original_loops.get_untracked());
        }
    });

    let scoring_for_navigation = scoring_engine.clone();
    let stop_for_navigation: Rc<dyn Fn()> = Rc::new(move || {
        if matches!(
            phase.get_untracked().as_str(),
            "running" | "count_in" | "starting" | "paused"
        ) {
            selection_changed.set(false);
            set_practice_active(false);
            stop_audio();
            observed.set(vec![]);
            feedback_dirty.set(true);
            accepted_observed.set(vec![]);
            practice_origin_ms.set(0.0);
            run_start_ms.set(0.0);
            paused_run_start_ms.set(None);
            bar_attempt.set(1);
            failed_repeat_attempts.set(vec![]);
            repeat_attempt_summaries.set(vec![]);
            scoring_for_navigation.borrow_mut().take();
            live.set(vec![]);
            live_matched_expected.set(HashSet::new());
            result.set(None);
            progress.set(-1.0);
            timeline_beat.set(-1.0);
            clear_score_feedback();
            phase.set("idle".into());
        }
        if adaptive_session_active.get_untracked() {
            adaptive_session_active.set(false);
            adaptive_bars.set(None);
            adaptive_status.set(None);

            bpm.set(adaptive_original_bpm.get_untracked());
            loops.set(adaptive_original_loops.get_untracked());
        }
    });
    let toggle_bar: Rc<dyn Fn(usize)> = {
        let pause = pause_run.clone();
        let save = save_current_settings.clone();
        Rc::new(move |bar| {
            if matches!(
                phase.get_untracked().as_str(),
                "running" | "count_in" | "starting"
            ) {
                pause();
            }
            if phase.get_untracked() == "paused" {
                selection_changed.set(true);
            }
            selected_bars.update(|bars| match bars.binary_search(&bar) {
                Ok(index) => {
                    bars.remove(index);
                }
                Err(index) => bars.insert(index, bar),
            });
            save();
        })
    };
    let toggle_bar = StoredValue::new_local(toggle_bar);
    {
        let toggle = toggle_bar.get_value();
        let selection = Closure::wrap(Box::new(move |bar: u32, _extend: bool| {
            toggle(bar as usize);
        }) as Box<dyn FnMut(u32, bool)>);
        bind_score_selection(selection.as_ref().unchecked_ref());
        selection.forget();
    }

    let scoring_for_tempo = scoring_engine.clone();
    let change_tempo: Rc<dyn Fn(f64)> = Rc::new(move |value| {
        if adaptive_session_active.get_untracked() {
            return;
        }
        if adaptive_enabled.get_untracked() {
            adaptive_original_bpm.set(value.clamp(30.0, 240.0));
        }
        let value = value.clamp(30.0, 240.0);
        bpm.set(value);
        if matches!(
            phase.get_untracked().as_str(),
            "running" | "count_in" | "paused"
        ) {
            let current_exercise = exercise.get_untracked();
            let beat = progress.get_untracked().max(0.0);
            let sequence = bar_sequence.get_untracked();
            let retimed = retime_tempo_map(
                &runtime_tempo_map.get_untracked(),
                &current_exercise,
                &sequence,
                if loops.get_untracked() == 0 {
                    CONTINUOUS_LOOP_CHUNK
                } else {
                    loops.get_untracked()
                },
                beat,
                value,
            );
            runtime_tempo_map.set(retimed.clone());
            let scoring_bpm = retimed.first().map(|change| change.bpm).unwrap_or(value);
            if let Some(scorer) = scoring_for_tempo.borrow_mut().as_mut() {
                scorer.retime(scoring_bpm, &retimed);
            }
            set_audio_tempo(value);
        }
    });

    let adaptive_controller_for_start = adaptive_controller.clone();
    let start_new_run: Rc<dyn Fn(u8)> = {
        let launch = launch_run.clone();
        let scoring_for_start = scoring_engine.clone();
        Rc::new(move |count_bars| {
            result_modal_open.set(false);
            selection_changed.set(false);
            if adaptive_enabled.get_untracked() {
                let current_phase = adaptive_controller_for_start
                    .borrow()
                    .as_ref()
                    .map(|controller| controller.phase());
                if !adaptive_session_active.get_untracked()
                    || current_phase == Some(AdaptivePhase::Completed)
                    || current_phase == Some(AdaptivePhase::Inactive)
                {
                    let current_exercise = exercise.get_untracked();
                    let sequence = bar_sequence.get_untracked();
                    let target_bpm = bpm.get_untracked();
                    let bars = sequence.bars.clone();
                    let total_bars = sequence.total_bars;
                    let counts = expected_notes_per_bar(&current_exercise);
                    let mut controller = AdaptiveController::new(
                        adaptive_settings.get_untracked(),
                        target_bpm,
                        bars.clone(),
                        total_bars,
                        counts,
                        sequence.bar_beats,
                    );
                    let decision = controller.start();
                    adaptive_original_bars.set(bars);
                    adaptive_original_bpm.set(target_bpm);
                    adaptive_original_loops.set(loops.get_untracked());
                    adaptive_controller_for_start
                        .borrow_mut()
                        .replace(controller);
                    adaptive_session_active.set(true);
                    adaptive_bars.set(Some(decision.bars.clone()));
                    bpm.set(decision.bpm);
                    loops.set(1);
                    repeat_bar_until_good.set(false);
                    adaptive_status.set(Some(AdaptiveStatus::from_decision(&decision)));
                }
            }
            let current_exercise = exercise.get_untracked();
            let sequence = bar_sequence.get_untracked();
            let start_beat = sequence.origin;
            app_mode.set(
                if sequence.bars.len() == sequence.total_bars
                    && loops.get_untracked() == 1
                    && !repeat_bar_until_good.get_untracked()
                    && !adaptive_enabled.get_untracked()
                {
                    AppMode::Performance
                } else {
                    AppMode::Practice
                },
            );
            let configured_loops = if loops.get_untracked() == 0 {
                CONTINUOUS_LOOP_CHUNK
            } else {
                loops.get_untracked()
            };
            run_expected.set(Arc::new(expanded_sequence_hits(
                &current_exercise,
                &sequence,
                configured_loops,
            )));
            observed.set(vec![]);
            feedback_dirty.set(true);
            accepted_observed.set(vec![]);
            practice_origin_ms.set(0.0);
            run_start_ms.set(0.0);
            paused_run_start_ms.set(None);
            bar_attempt.set(1);
            failed_repeat_attempts.set(vec![]);
            repeat_attempt_summaries.set(vec![]);
            scoring_for_start.borrow_mut().take();
            live.set(vec![]);
            live_matched_expected.set(HashSet::new());
            result.set(None);
            progress.set(-1.0);
            timeline_beat.set(-1.0);
            clear_score_feedback();
            scroll_score_to_beat(start_beat);
            launch(
                start_beat,
                f64::from(count_bars) * beats_per_bar(&current_exercise),
                false,
            );
        })
    };
    *start_holder.borrow_mut() = Some(start_new_run.clone());

    let adaptive_advance = {
        let start = start_new_run.clone();
        let controller = adaptive_controller.clone();
        let bpm_signal = bpm;
        let adaptive_bars_signal = adaptive_bars;
        let adaptive_status_signal = adaptive_status;
        let adaptive_session_signal = adaptive_session_active;
        let count_in_signal = count_in;
        Rc::new(move |summary: SessionSummary| {
            let mut controller_ref = controller.borrow_mut();
            let Some(controller) = controller_ref.as_mut() else {
                return;
            };
            let performed = bar_sequence.get_untracked().bars.clone();
            let current_bpm = bpm_signal.get_untracked();
            let decision = controller.observe(&AdaptiveAttempt {
                summary: summary.clone(),
                performed_bars: performed,
                current_bpm,
            });
            adaptive_status_signal.set(Some(AdaptiveStatus::from_decision(&decision)));
            if decision.completed {
                adaptive_session_signal.set(false);
                adaptive_bars_signal.set(None);
                bpm_signal.set(decision.bpm);
                let record = SessionRecord {
                    id: format!("adaptive-{}", Date::now()),
                    ended_at: Date::new_0().to_iso_string().into(),
                    exercise_id: exercise.get_untracked().id.clone(),
                    exercise_title: exercise.get_untracked().title.clone(),
                    exercise_hash: exercise.get_untracked().content_hash.clone(),
                    bpm: decision.bpm,
                    input_mode: input_mode.get_untracked(),
                    scoring_version: SCORING_VERSION.into(),
                    tolerance_ms: on_time_ms.get_untracked(),
                    latency_ms: latency_ms.get_untracked(),
                    summary,
                    observed: vec![],
                    schema_version: 1,
                    mode: AppMode::Practice,
                };
                result.set(Some(record));
                phase.set("complete".into());
                result_modal_open.set(true);
                result_dismiss_after_ms.set(f64::INFINITY);
                return;
            }
            bpm_signal.set(decision.bpm);
            adaptive_bars_signal.set(Some(decision.bars.clone()));
            drop(controller_ref);
            let start = start.clone();
            let count = count_in_signal.get_untracked();
            spawn_local(async move {
                start(count);
            });
        })
    };
    *adaptive_advance_holder.borrow_mut() = Some(adaptive_advance);

    let save_for_hit = save_current_settings.clone();
    let start_for_hit = start_new_run.clone();
    let scoring_for_hit = scoring_engine.clone();
    let hit_input: HitInput = Rc::new(
        move |timestamp, instrument, articulation, velocity, source, raw_note, raw_channel| {
            if result_modal_open.get_untracked() {
                if timestamp >= result_dismiss_after_ms.get_untracked() {
                    dismiss_result(
                        result_modal_open,
                        result_dismiss_after_ms,
                        historical_result,
                        stats_open,
                    );
                }
                return;
            }
            if !snare_muted.get_untracked() && snare_reference_mode.get_untracked() == "hits" {
                play_drum(
                    instrument_key(instrument),
                    articulation_key(articulation),
                    velocity,
                );
            }
            input_mode.set(source);
            if library_open.get_untracked() {
                pending_import_delete.set(None);
                library_open.set(false);
                restore_library_focus();
                return;
            }
            if stats_open.get_untracked() {
                stats_open.set(false);
                restore_stats_focus();
                return;
            }
            if !calibration_targets.get_untracked().is_empty() {
                let targets = calibration_targets.get_untracked();
                let used = calibration_errors.get_untracked().len();
                if let Some(target) = targets.get(used) {
                    let error = timestamp - *target;
                    calibration_errors.update(|items| items.push(error));
                    if used + 1 >= 8 {
                        let mut errors = calibration_errors.get_untracked();
                        errors.sort_by(|a, b| a.total_cmp(b));
                        let median = (errors[3] + errors[4]) / 2.0;
                        latency_ms.set(median);
                        calibration_targets.set(vec![]);
                        save_for_hit();
                    }
                }
                return;
            }
            if settings_open.get_untracked() || context_popup.get_untracked().is_some() {
                return;
            }
            if matches!(phase.get_untracked().as_str(), "idle" | "complete") {
                input_mode.set(source);
                start_for_hit(count_in.get_untracked());
                return;
            }
            let phase_now = phase.get_untracked();
            if phase_now != "running" {
                if !matches!(phase_now.as_str(), "count_in" | "starting") {
                    return;
                }
                // The first downbeat may be struck slightly early, before the
                // next animation frame changes count-in to running.
                let first_beat = if repeat_bar_until_good.get_untracked() {
                    active_segment_start.get_untracked()
                } else {
                    bar_sequence.get_untracked().origin
                };
                let tempo_map = runtime_tempo_map.get_untracked();
                let scoring_bpm = tempo_map
                    .first()
                    .map(|change| change.bpm)
                    .unwrap_or(bpm.get_untracked());
                let first_hit_ms = run_start_ms.get_untracked()
                    + elapsed_ms_at_beat(first_beat, scoring_bpm, &tempo_map)
                    + latency_ms.get_untracked();
                if run_start_ms.get_untracked() <= 0.0
                    || timestamp < first_hit_ms - DEFAULT_MATCH_MS
                {
                    return;
                }
                phase.set("running".into());
            }
            let event = ObservedHit {
                timestamp_ms: timestamp,
                instrument,
                articulation,
                velocity,
                source,
                raw_note,
                raw_channel,
            };
            let session_start = run_start_ms.get_untracked();
            let tempo_map = runtime_tempo_map.get_untracked();
            let scoring_bpm = tempo_map
                .first()
                .map(|change| change.bpm)
                .unwrap_or(bpm.get_untracked());
            let sequence = bar_sequence.get_untracked();
            let scoring_result = {
                let mut scoring = scoring_for_hit.borrow_mut();
                let Some(scorer) = scoring.as_mut() else {
                    import_error.set(Some(
                        "Scoring was not ready for this strike; restart the attempt.".into(),
                    ));
                    phase.set("error".into());
                    stop_audio();
                    return;
                };
                scorer
                    .observe(event.clone(), timestamp - latency_ms.get_untracked())
                    .and_then(|_| scorer.take_feedback_update())
            };
            match scoring_result {
                Ok(feedback) => {
                    observed.update(|items| items.push(event));
                    let mut update = live_feedback_from_scoring(
                        &feedback,
                        &sequence,
                        scoring_bpm,
                        &tempo_map,
                        session_start,
                        latency_ms.get_untracked(),
                    );
                    if !repeat_bar_until_good.get_untracked() {
                        let iteration = visible_loop_index.get_untracked();
                        update.retain(|judgement| {
                            judgement.iteration.is_none_or(|value| value >= iteration)
                        });
                    }
                    let mut visible = live.get_untracked();
                    merge_live_feedback(&mut visible, update);
                    if !repeat_bar_until_good.get_untracked() {
                        let iteration = visible_loop_index.get_untracked();
                        visible.retain(|judgement| {
                            judgement.iteration.is_none_or(|value| value == iteration)
                        });
                    }
                    let judgement_values =
                        serde_wasm_bindgen::to_value(&visible).unwrap_or(JsValue::NULL);
                    live.set(visible);
                    // Keep the authoritative projection in Rust in this input
                    // turn, but coalesce DOM work at the browser's next paint.
                    // This queue is independent of Tone's transport frame, so
                    // pausing immediately after a hit cannot strand feedback.
                    queue_score_feedback(judgement_values);
                    feedback_dirty.set(false);
                }
                Err(error) => {
                    set_practice_active(false);
                    phase.set("error".into());
                    import_error.set(Some(format!(
                        "Scoring stopped to preserve reliable results: {error}"
                    )));
                    stop_audio();
                }
            }
        },
    );

    let scoring_for_file = scoring_engine.clone();
    let file_callback: Rc<MidiFileCallback> = Rc::new(Closure::wrap(Box::new(
        move |bytes: Uint8Array, name: String, error: String, save_to_library: JsValue| {
            if library_open.get_untracked() {
                pending_import_delete.set(None);
                library_open.set(false);
                restore_library_focus();
            }
            if matches!(
                phase.get_untracked().as_str(),
                "running" | "count_in" | "starting" | "paused"
            ) {
                set_practice_active(false);
                stop_audio();
                observed.set(vec![]);
                accepted_observed.set(vec![]);
                practice_origin_ms.set(0.0);
                paused_run_start_ms.set(None);
                bar_attempt.set(1);
                failed_repeat_attempts.set(vec![]);
                repeat_attempt_summaries.set(vec![]);
                scoring_for_file.borrow_mut().take();
                live.set(vec![]);
                live_matched_expected.set(HashSet::new());
                progress.set(-1.0);
                timeline_beat.set(-1.0);
                clear_score_feedback();
                phase.set("idle".into());
                if adaptive_session_active.get_untracked() {
                    adaptive_session_active.set(false);
                    adaptive_bars.set(None);
                    adaptive_status.set(None);
                }
            }
            if !error.is_empty() {
                import_error.set(Some(error));
                return;
            }
            let save_to_library = save_to_library.as_bool().unwrap_or(true);
            let data = bytes.to_vec();
            match parse_midi(&data, &name, None) {
                Ok(parsed) => {
                    if !restoring_import.get_untracked() {
                        bpm.set(parsed.tempo_bpm.clamp(30.0, 240.0).round());
                    }
                    import_error.set(None);
                    imported_bytes.set(data.clone());
                    imported_name.set(name.clone());
                    let metadata = serde_wasm_bindgen::to_value(&parsed).unwrap_or(JsValue::NULL);
                    let bytes_copy = Uint8Array::from(data.as_slice());
                    let hash = parsed.content_hash.clone();
                    if save_to_library {
                        selected_import.set(Some(hash.clone()));
                        let last_hash = hash.clone();
                        spawn_local(async move {
                            if let Err(error) =
                                JsFuture::from(save_last_import(JsValue::from_str(&last_hash)))
                                    .await
                            {
                                import_error.set(Some(format!(
                                    "MIDI loaded, but the last-selected setting could not be saved: {}",
                                    error.as_string().unwrap_or_else(|| format!("{error:?}"))
                                )));
                            }
                            match JsFuture::from(save_import(&hash, &name, &bytes_copy, metadata)).await
                            {
                                Ok(_) => match JsFuture::from(load_imports()).await {
                                    Ok(value) => match serde_wasm_bindgen::from_value::<Vec<SavedMidiImport>>(value) {
                                        Ok(mut items) => {
                                            items.sort_by_key(|item| item.name.to_lowercase());
                                            saved_imports.set(items);
                                        }
                                        Err(error) => import_error.set(Some(format!(
                                            "MIDI loaded, but the library could not be refreshed: {error}"
                                        ))),
                                    },
                                    Err(error) => import_error.set(Some(format!(
                                        "MIDI loaded, but the library could not be refreshed: {error:?}"
                                    ))),
                                },
                                Err(error) => import_error.set(Some(format!(
                                    "MIDI loaded for this session, but could not be saved to the local library: {}",
                                    error.as_string().unwrap_or_else(|| format!("{error:?}"))
                                ))),
                            }
                        });
                    } else {
                        selected_import.set(None);
                        spawn_local(async move {
                            if let Err(error) =
                                JsFuture::from(save_last_import(JsValue::NULL)).await
                            {
                                import_error.set(Some(format!(
                                    "Bundled MIDI loaded, but the last-selected setting could not be cleared: {}",
                                    error.as_string().unwrap_or_else(|| format!("{error:?}"))
                                )));
                            }
                        });
                    }
                    context_popup.set(None);
                    selection_changed.set(false);
                    if let Some((hash, start, end)) = pending_legacy_selection.get_untracked()
                        && hash == parsed.content_hash
                    {
                        selected_bars.set(legacy_selected_bars(&parsed, start, end));
                        pending_legacy_selection.set(None);
                    } else {
                        let count = (exercise_beats(&parsed) / beats_per_bar(&parsed))
                            .ceil()
                            .max(1.0) as usize;
                        selected_bars.update(|bars| bars.retain(|bar| *bar < count));
                    }
                    exercise.set(parsed);
                    result_modal_open.set(false);
                    phase.set("idle".into());
                }
                Err(err) => import_error.set(Some(err.to_string())),
            }
        },
    )
        as Box<dyn FnMut(Uint8Array, String, String, JsValue)>));

    let save_for_midi = save_current_settings.clone();
    let midi_hit = hit_input.clone();
    let midi_pause = pause_run.clone();
    let midi_callback: Rc<Closure<dyn FnMut(JsValue)>> = Rc::new(Closure::wrap(Box::new(
        move |value: JsValue| {
            let Ok(event) = serde_wasm_bindgen::from_value::<MidiEnvelope>(value) else {
                return;
            };
            if event.kind == "state" {
                midi_ports.set(event.ports.clone());
                selected_midi.set(event.selected_id.clone());
                midi_permission.set(event.permission.clone());
                if let Some(error) = event.error {
                    midi_status.set(format!("MIDI error · {error}"));
                } else {
                    if event.disconnected_id.is_some()
                        && matches!(
                            phase.get_untracked().as_str(),
                            "running" | "count_in" | "starting"
                        )
                    {
                        midi_pause();
                    }
                    if let Some(id) = event.selected_id {
                        let port = event.ports.iter().find(|port| port.id == id);
                        midi_status.set(
                            port.map(|port| format!("{} · connected", port.name))
                                .unwrap_or_else(|| "Connected".into()),
                        );
                    } else if event.disconnected_id.is_some() {
                        midi_status.set("Drums disconnected — reconnecting when available".into());
                    } else if event.permission.as_deref() == Some("unsupported") {
                        midi_status.set("Web MIDI unavailable in this browser".into());
                    } else if !event.ports.is_empty() {
                        midi_status.set("Choose a MIDI input".into());
                    } else {
                        midi_status.set("No MIDI drums detected".into());
                    }
                }
                return;
            }
            let line = format!(
                "{:.3} ms · {} ch{} · note {} · velocity {} · raw {:?}",
                event.timestamp.unwrap_or(0.0),
                event.message_type.clone().unwrap_or_default(),
                event.channel.unwrap_or(0),
                event.note.unwrap_or(0),
                event.velocity.unwrap_or(0),
                event.raw_bytes
            );
            diagnostics.update(|items| {
                items.insert(0, line);
                items.truncate(12);
            });
            if event.message_type.as_deref() != Some("note_on") {
                return;
            }
            let note = event.note.unwrap_or(0);
            let channel = event.channel.unwrap_or(1);
            let velocity = event.velocity.unwrap_or(0);
            if velocity == 0 {
                return;
            }
            if let Some(target) = midi_learn.get_untracked() {
                let binding = MidiBinding {
                    note,
                    channel,
                    instrument: target,
                    articulation: learned_articulation(target),
                };
                midi_bindings.update(|items| {
                    items.retain(|item| item.instrument != target);
                    items.push(binding.clone());
                });
                if target == Instrument::Snare {
                    snare_note.set(note);
                    snare_channel.set(channel);
                }
                midi_learn.set(None);
                midi_status.set(format!(
                    "{} learned · note {note}, channel {channel}",
                    instrument_label(target)
                ));
                let profile_id = event
                    .input_id
                    .clone()
                    .unwrap_or_else(|| "user-device".into());
                let bindings = midi_bindings.get_untracked();
                midi_profiles.update(|profiles| {
                    profiles.retain(|profile| profile.id != profile_id);
                    profiles.push(SavedMidiProfile {
                        id: profile_id.clone(),
                        bindings: bindings.clone(),
                    });
                });
                let profile = serde_json::json!({"id":profile_id,"name":event.input_name.unwrap_or_else(||"MIDI drums".into()),"schemaVersion":1,"verified":true,"bindings":bindings,"latencyMs":latency_ms.get_untracked()});
                if let Ok(value) = json_compatible(&profile) {
                    spawn_local(async move {
                        let _ = JsFuture::from(save_profile(value)).await;
                    });
                }
                save_for_midi();
                return;
            }
            let (instrument, articulation) =
                classify_midi(note, channel, &midi_bindings.get_untracked());
            midi_hit(
                event.timestamp.unwrap_or_else(|| {
                    web_sys::window()
                        .and_then(|window| window.performance())
                        .map(|performance| performance.now())
                        .unwrap_or(0.0)
                }),
                instrument,
                articulation,
                velocity,
                InputSource::Midi,
                Some(note),
                Some(channel),
            );
        },
    )
        as Box<dyn FnMut(JsValue)>));

    {
        let file_callback = file_callback.clone();
        Effect::new(move |_| {
            if let Ok(value) = serde_wasm_bindgen::to_value(&exercise.get()) {
                render_score(value);
            }
            let callback = file_callback.as_ref().as_ref().unchecked_ref::<Function>();
            bind_drop(callback, MAX_MIDI_FILE_BYTES);
        });
    }
    Effect::new(move |_| {
        let _ = exercise.get();
        if let Ok(value) = json_compatible(&selected_bars.get()) {
            set_selected_bars(value);
        }
    });
    {
        let callback = midi_callback.clone();
        spawn_local(async move {
            let _ = JsFuture::from(auto_connect_midi(
                callback.as_ref().as_ref().unchecked_ref(),
            ))
            .await;
        });
    }
    // Startup MIDI setup moved to the dedicated onboarding page: it renders
    // immediately on arrival and is not driven by a delayed timer. Manual MIDI
    // configuration remains available from the settings drawer.
    {
        let callback = file_callback.clone();
        spawn_local(async move {
            match JsFuture::from(load_imports()).await {
                Ok(value) => match serde_wasm_bindgen::from_value::<Vec<SavedMidiImport>>(value) {
                    Ok(mut items) => {
                        items.sort_by_key(|item| item.name.to_lowercase());
                        saved_imports.set(items);
                    }
                    Err(error) => import_error
                        .set(Some(format!("Saved MIDI library could not load: {error}"))),
                },
                Err(error) => import_error.set(Some(format!(
                    "Saved MIDI library storage failed: {}",
                    error.as_string().unwrap_or_else(|| format!("{error:?}"))
                ))),
            }
            let Ok(value) = JsFuture::from(load_settings()).await else {
                return;
            };
            let Ok(settings) = serde_wasm_bindgen::from_value::<Settings>(value) else {
                return;
            };
            let Some(hash) = settings.last_import_hash else {
                return;
            };
            match JsFuture::from(load_import(&hash)).await {
                Ok(value) if !value.is_null() && !value.is_undefined() => {
                    match serde_wasm_bindgen::from_value::<SavedMidiImport>(value) {
                        Ok(saved) => {
                            selected_import.set(Some(saved.hash));
                            let function = callback.as_ref().as_ref().unchecked_ref::<Function>();
                            restoring_import.set(true);
                            let _ = function.call3(
                                &JsValue::NULL,
                                &Uint8Array::from(saved.bytes.as_slice()),
                                &JsValue::from_str(&saved.name),
                                &JsValue::from_str(""),
                            );
                            restoring_import.set(false);
                            // Re-apply the persisted drum-track choice once the
                            // file is back; the restore callback parses the
                            // default track.
                            if let Some(track) = settings.selected_track
                                && track != exercise.get_untracked().selected_track
                                && let Ok(parsed) = parse_midi(
                                    &imported_bytes.get_untracked(),
                                    &imported_name.get_untracked(),
                                    Some(track),
                                )
                            {
                                exercise.set(parsed);
                            }
                        }
                        Err(error) => import_error.set(Some(format!(
                            "The last MIDI file could not be restored: {error}"
                        ))),
                    }
                }
                Ok(_) => import_error.set(Some(
                    "The last selected MIDI file is no longer in the local library.".into(),
                )),
                Err(error) => import_error.set(Some(format!(
                    "The last MIDI file could not be restored: {}",
                    error.as_string().unwrap_or_else(|| format!("{error:?}"))
                ))),
            }
        });
    }
    spawn_local(async move {
        if let Ok(value) = JsFuture::from(load_sessions()).await {
            let records = Array::from(&value);
            let mut items = records
                .iter()
                .filter_map(|record| serde_wasm_bindgen::from_value::<SessionRecord>(record).ok())
                .collect::<Vec<_>>();
            incomplete_history.set(records.length() as usize - items.len());
            items.sort_by(|left, right| {
                left.ended_at
                    .cmp(&right.ended_at)
                    .then_with(|| left.id.cmp(&right.id))
            });
            history.set(items);
        }
    });
    spawn_local(async move {
        if let Ok(value) = JsFuture::from(load_profiles()).await {
            midi_profiles.set(
                Array::from(&value)
                    .iter()
                    .filter_map(|profile| {
                        serde_wasm_bindgen::from_value::<SavedMidiProfile>(profile).ok()
                    })
                    .collect(),
            );
        }
    });
    spawn_local(async move {
        match JsFuture::from(load_settings()).await {
            Ok(value) => match serde_wasm_bindgen::from_value::<Settings>(value) {
                Ok(saved) => {
                    if let Some(v) = saved.mode {
                        app_mode.set(v);
                    }
                    if let Some(v) = saved.bpm {
                        bpm.set(v);
                    }
                    if let Some(v) = saved.count_in_bars {
                        count_in.set(v);
                    }
                    if let Some(v) = saved.subdivision {
                        subdivision.set(v);
                    }
                    if let Some(v) = saved.timbre {
                        timbre.set(v);
                    }
                    if let Some(v) = saved.metronome {
                        metronome.set(v);
                    }
                    if let Some(v) = saved.auto_show_statistics {
                        auto_show_statistics.set(v);
                    }
                    if let Some(v) = saved.click_volume {
                        click_volume.set(v);
                    }
                    if let Some(v) = saved.snare_volume {
                        snare_volume.set(v);
                    }
                    if let Some(v) = saved.snare_muted {
                        snare_muted.set(v);
                    }
                    if matches!(
                        saved.snare_reference_mode.as_deref(),
                        Some("hits" | "notes")
                    ) {
                        snare_reference_mode.set(saved.snare_reference_mode.unwrap());
                    }
                    if let Some(v) = saved.loops {
                        loops.set(v);
                    }
                    if let Some(v) = saved.repeat_bar_until_good {
                        repeat_bar_until_good.set(v);
                    }
                    if let Some(v) = saved.repeat_tolerance {
                        repeat_tolerance.set(v);
                    }
                    if let Some(v) = saved.adaptive_enabled {
                        adaptive_enabled.set(v);
                    }
                    if let Some(v) = saved.adaptive {
                        adaptive_settings.set(v);
                    }
                    if let Some(mut bars) = saved.selected_bars {
                        bars.sort_unstable();
                        bars.dedup();
                        selected_bars.set(bars);
                    } else if let (Some(start), Some(end)) =
                        (saved.selection_start_bar, saved.selection_end_bar)
                    {
                        if let Some(hash) = saved
                            .last_import_hash
                            .filter(|hash| selected_import.get_untracked().as_ref() != Some(hash))
                        {
                            pending_legacy_selection.set(Some((hash, start, end)));
                        } else {
                            selected_bars.set(legacy_selected_bars(
                                &exercise.get_untracked(),
                                start,
                                end,
                            ));
                        }
                    }
                    if let Some(v) = saved.latency_ms {
                        latency_ms.set(v);
                    }
                    if let Some(v) = saved.on_time_ms {
                        on_time_ms.set(v);
                    }
                    if let Some(v) = saved.excellent_ms {
                        excellent_ms.set(v);
                    }
                    if let Some(v) = saved.snare_note {
                        snare_note.set(v);
                    }
                    if let Some(v) = saved.snare_channel {
                        snare_channel.set(v);
                    }
                    if let Some(bindings) = saved.midi_bindings.filter(|items| !items.is_empty()) {
                        midi_bindings.set(bindings);
                    } else if saved.snare_note.is_some() || saved.snare_channel.is_some() {
                        let mut bindings = default_midi_bindings();
                        if let Some(snare) = bindings
                            .iter_mut()
                            .find(|binding| binding.instrument == Instrument::Snare)
                        {
                            snare.note = snare_note.get_untracked();
                            snare.channel = snare_channel.get_untracked();
                        }
                        midi_bindings.set(bindings);
                    }
                    welcome.set(!saved.welcome_dismissed.unwrap_or(false));
                    if let Some(difficulty) = saved
                        .library_difficulty
                        .as_deref()
                        .and_then(parse_difficulty_label)
                    {
                        library_difficulty.set(Some(difficulty));
                    }
                    set_volumes(
                        if snare_muted.get_untracked() {
                            0.0
                        } else {
                            snare_volume.get_untracked()
                        },
                        click_volume.get_untracked(),
                    );
                    settings_loaded.set(true);
                }
                Err(error) => {
                    settings_loaded.set(true);
                    import_error.set(Some(format!("Saved settings could not load: {error}")))
                }
            },
            Err(error) => {
                settings_loaded.set(true);
                import_error.set(Some(format!("Saved settings storage failed: {error:?}")))
            }
        }
    });

    let key_reset_run = reset_run.clone();
    let key_start_new_run = start_new_run.clone();
    let key_hit = hit_input.clone();
    let _key_pause_run = pause_run.clone();
    let _key_resume_run = resume_run.clone();
    // Capture activation before component handlers run. Transport and restart
    // own their toggle/reset semantics; loading a file still replaces the run.
    let interaction_pause = pause_run.clone();
    let control_interaction = Closure::wrap(Box::new(move |event: web_sys::Event| {
        if !matches!(
            phase.get_untracked().as_str(),
            "running" | "count_in" | "starting"
        ) {
            return;
        }
        let control = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
            .and_then(|element| {
                element
                    .closest("button, input, select, textarea, summary, a[href]")
                    .ok()
                    .flatten()
            });
        if let Some(control) = control
            && control
                .closest(".start-button, .restart-button, .notation-theme-toggle")
                .ok()
                .flatten()
                .is_none()
        {
            interaction_pause();
        }
    }) as Box<dyn FnMut(web_sys::Event)>);
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        for name in ["click", "pointerdown", "change"] {
            let _ = document.add_event_listener_with_callback_and_bool(
                name,
                control_interaction.as_ref().unchecked_ref(),
                true,
            );
        }
    }
    control_interaction.forget();
    let outside_context_popup = Closure::wrap(Box::new(move |event: web_sys::Event| {
        let target = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Element>().ok());
        let inside_file_info = target
            .as_ref()
            .and_then(|element| element.closest(".score-file-info").ok().flatten())
            .is_some();
        if !inside_file_info
            && let Some(details) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| {
                    document
                        .query_selector(".score-file-info[open]")
                        .ok()
                        .flatten()
                })
        {
            let _ = details.remove_attribute("open");
        }
        let Some(popup) = context_popup.get_untracked() else {
            return;
        };
        let selector = match popup {
            ContextPopup::Bars => ".bar-selection-control",
            ContextPopup::Metronome => ".metronome-context",
        };
        let inside = target
            .and_then(|element| element.closest(selector).ok().flatten())
            .is_some();
        if !inside {
            context_popup.set(None);
        }
    }) as Box<dyn FnMut(web_sys::Event)>);
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        let _ = document.add_event_listener_with_callback(
            "click",
            outside_context_popup.as_ref().unchecked_ref(),
        );
    }
    outside_context_popup.forget();
    let keyboard = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
        // A held computer key generates synthetic keydown events at the OS
        // repeat interval. Those are not additional physical drum strikes and
        // otherwise form a misleading, evenly spaced row of unmatched hits.
        if event.repeat() {
            return;
        }
        if let Some(popup) = context_popup.get_untracked()
            && event.key() == "Escape"
        {
            event.prevent_default();
            context_popup.set(None);
            focus_popup_trigger(popup);
            return;
        }
        if event.key() == "Escape"
            && let Some(details) = web_sys::window()
                .and_then(|window| window.document())
                .and_then(|document| {
                    document
                        .query_selector(".score-file-info[open]")
                        .ok()
                        .flatten()
                })
        {
            event.prevent_default();
            let _ = details.remove_attribute("open");
            if let Some(summary) = details.query_selector("summary").ok().flatten()
                && let Ok(summary) = summary.dyn_into::<web_sys::HtmlElement>()
            {
                let _ = summary.focus();
            }
            return;
        }
        if event.default_prevented() {
            return;
        }
        if let Some((hash, _)) = pending_import_delete.get_untracked() {
            match event.key().to_lowercase().as_str() {
                "escape" => {
                    event.prevent_default();
                    event.stop_immediate_propagation();
                    dismiss_import_delete(pending_import_delete, hash);
                }
                "tab" if !event.default_prevented() => {
                    trap_dialog_tab(&event, "#import-delete-dialog");
                }
                _ => {}
            }
            return;
        }
        if library_open.get_untracked() {
            if event.key() == "Escape" {
                event.prevent_default();
                pending_import_delete.set(None);
                library_open.set(false);
                restore_library_focus();
            } else if matches!(event.key().to_lowercase().as_str(), "j" | "r")
                || (event.key() == " " && !keyboard_targets_control(&event))
            {
                event.prevent_default();
            }
            return;
        }
        if stats_open.get_untracked() {
            match event.key().to_lowercase().as_str() {
                "tab" => {
                    // Focus can be on the page body after a programmatic blur.
                    // The dialog-local handler cannot see that event, so keep
                    // keyboard navigation inside the open modal here as well.
                    if !event.default_prevented() {
                        trap_dialog_tab(&event, "#stats-dialog");
                    }
                    return;
                }
                "escape" => {
                    event.prevent_default();
                    stats_open.set(false);
                    restore_stats_focus();
                    return;
                }
                "j" => {}
                " " if keyboard_targets_control(&event) => return,
                " " | "r" => {
                    event.prevent_default();
                    return;
                }
                _ => return,
            }
        }
        if result_modal_open.get_untracked() {
            match event.key().to_lowercase().as_str() {
                "escape" => {
                    event.prevent_default();
                    dismiss_result(
                        result_modal_open,
                        result_dismiss_after_ms,
                        historical_result,
                        stats_open,
                    );
                    return;
                }
                "j" => {}
                " " if keyboard_targets_control(&event) => return,
                " " | "r" => {
                    event.prevent_default();
                    return;
                }
                _ => return,
            }
        }
        if settings_open.get_untracked() && event.key().to_lowercase() != "j" {
            return;
        }
        let target = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlElement>().ok());
        if event.key() == " " && keyboard_targets_control(&event) {
            return;
        }
        let typing = target
            .as_ref()
            .map(|element| matches!(element.tag_name().as_str(), "INPUT" | "SELECT" | "TEXTAREA"))
            .unwrap_or(false);
        if typing {
            return;
        }
        match event.key().to_lowercase().as_str() {
            "j" => {
                event.prevent_default();
                key_hit(
                    web_sys::window().unwrap().performance().unwrap().now(),
                    Instrument::Snare,
                    Articulation::Center,
                    108,
                    InputSource::Demo,
                    None,
                    None,
                );
            }
            " " => {
                event.prevent_default();
                match phase.get_untracked().as_str() {
                    "idle" | "complete" => {
                        observed.set(vec![]);
                        live.set(vec![]);
                        result.set(None);
                        key_start_new_run(count_in.get_untracked());
                    }
                    _ => {}
                }
            }
            "r" => {
                event.prevent_default();
                key_reset_run();
            }
            _ => {}
        }
    }) as Box<dyn FnMut(web_sys::KeyboardEvent)>);
    let _ = web_sys::window()
        .unwrap()
        .add_event_listener_with_callback("keydown", keyboard.as_ref().unchecked_ref());
    keyboard.forget();

    let visibility_pause = pause_run.clone();
    let visibility = Closure::wrap(Box::new(move || {
        if !matches!(
            phase.get_untracked().as_str(),
            "running" | "count_in" | "starting"
        ) {
            return;
        }
        let hidden = web_sys::window()
            .and_then(|window| window.document())
            .map(|document| document.visibility_state() == web_sys::VisibilityState::Hidden)
            .unwrap_or(false);
        // Background policy: pause when the page is hidden. On returning, also
        // recover a run the OS froze mid-flight. Mobile browsers suspend the
        // output AudioContext in the background, and the transport clock
        // derives from that context, so a returned page can stay nominally
        // "playing" while silent and visually frozen: the animation frames are
        // live again but never reach the run's end. A clean pause gives the
        // correct Play button and a defined resume position; the resume path
        // re-unlocks the context and restarts from there.
        if hidden || !audio_output_running() {
            visibility_pause();
        }
    }) as Box<dyn FnMut()>);
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        let _ = document.add_event_listener_with_callback(
            "visibilitychange",
            visibility.as_ref().unchecked_ref(),
        );
    }
    visibility.forget();

    let button_pause = pause_run.clone();
    let button_resume = resume_run.clone();
    let connect_cb = midi_callback.clone();
    let connect_action: Rc<dyn Fn()> = {
        let callback = connect_cb.clone();
        Rc::new(move || {
            midi_status.set("Requesting permission…".into());
            let audio_promise = ensure_audio();
            let midi_promise = connect_midi(callback.as_ref().as_ref().unchecked_ref());
            spawn_local(async move {
                match JsFuture::from(audio_promise).await {
                    Ok(_) => {
                        set_volumes(
                            if snare_muted.get_untracked() {
                                0.0
                            } else {
                                snare_volume.get_untracked()
                            },
                            click_volume.get_untracked(),
                        );
                    }
                    Err(error) => {
                        let detail = error.as_string().unwrap_or_else(|| format!("{error:?}"));
                        import_error.set(Some(format!("Audio failed: {detail}")));
                    }
                }
                let _ = JsFuture::from(midi_promise).await;
            });
        })
    };
    let choose_cb = file_callback.clone();
    let stop_for_import = stop_for_navigation.clone();
    let save_welcome = save_current_settings.clone();
    let save_snare_mute = save_current_settings.clone();
    let save_snare_reference_mode = save_current_settings.clone();
    let save_snare_volume = save_current_settings.clone();
    let save_loops = save_current_settings.clone();
    let save_repeat_bar = save_current_settings.clone();
    let save_repeat_tolerance = save_current_settings.clone();
    let save_selection_all = save_current_settings.clone();
    let save_cal_reset = save_current_settings.clone();
    let save_cal_manual = save_current_settings.clone();
    let save_on_time = save_current_settings.clone();
    let save_excellent = save_current_settings.clone();
    let save_auto_statistics = save_current_settings.clone();
    // Entering the practice workspace closes onboarding for this session. The
    // opt-out is persisted only when "Don't show me this again" is checked.
    let enter_practice: Rc<dyn Fn()> = Rc::new(move || {
        if onboarding_skip.get_untracked() {
            save_onboarding_skip(true);
        }
        onboarding_open.set(false);
    });
    let enter_practice_action = enter_practice.clone();
    let enter_without_midi = enter_practice.clone();
    view! {
      <section id="onboarding" class=move || if onboarding_open.get(){"onboarding"}else{"onboarding hidden"} aria-labelledby="onboarding-title">
        <div class="onboarding-card">
          <span class="eyebrow">"WELCOME TO RUDIMENT"</span>
          <h1 id="onboarding-title">"Let's get you set up."</h1>
          <p class="onboarding-intro">"Pick how you want the music displayed and connect your drum kit if you have one. You can always change these settings in the Settings drawer."</p>
          <fieldset class="onboarding-display">
            <legend>"Practice display"</legend>
            <div class="onboarding-display-options">
              <button type="button" class=move || if !highway.get(){"active"}else{""} aria-pressed=move || (!highway.get()).to_string() on:click=move |_|{highway.set(false);save_highway_mode(false);}>
                <strong>"Notation"</strong>
                <span>"A written drum score, like sheet music"</span>
              </button>
              <button type="button" class=move || if highway.get(){"active"}else{""} aria-pressed=move || highway.get().to_string() on:click=move |_|{highway.set(true);save_highway_mode(true);}>
                <strong>"Highway"</strong>
                <span>"Scrolling lanes, like a rhythm game"</span>
              </button>
            </div>
          </fieldset>
          <section class="onboarding-midi">
            <h2>"MIDI drums"</h2>
            <p class="help">"Optional. Connect your kit now, or continue without MIDI and use keyboard or demo input. If your browser asks to use MIDI, allow it so Rudiment can see your drums."</p>
            <MidiConfigSection midi_status=midi_status midi_ports=midi_ports selected_midi=selected_midi midi_learn=midi_learn midi_bindings=midi_bindings diagnostics=diagnostics midi_modal_open=midi_modal_open connect=connect_action.clone()/>
          </section>
          <label class="toggle-row onboarding-skip"><input type="checkbox" prop:checked=move || onboarding_skip.get() on:change=move |event|onboarding_skip.set(event_target_checked(&event))/><span>"Don't show me this again"</span></label>
          <div class="onboarding-actions">
            <button id="onboarding-enter" class="primary" on:click=move |_|enter_practice_action()>"Enter practice workspace"</button>
            <button id="onboarding-without-midi" on:click=move |_|enter_without_midi()>"Continue without MIDI"</button>
          </div>
        </div>
      </section>
      <main class="app" inert=move || onboarding_open.get()>
        <header class="topbar">
          <button id="midi-library-trigger" class="stats-trigger library-topbar-trigger" aria-label=move || format!("MIDI library · {} exercises",saved_imports.get().len()+BUILTIN_RUDIMENTS.len()) title=move || format!("MIDI library · {} exercises",saved_imports.get().len()+BUILTIN_RUDIMENTS.len()) aria-haspopup="dialog" aria-expanded=move || library_open.get().to_string() on:click=move |_|{
            pending_import_delete.set(None);
            library_personal.set(false);
            library_page.set(0);
            library_open.set(true);
            focus_library_close();
          }><FolderOpen size=19/><span class="library-count" aria-hidden="true">{move || saved_imports.get().len()+BUILTIN_RUDIMENTS.len()}</span></button>
          <button id="stats-trigger" class="stats-trigger" aria-label="Stats" title="Performance history" aria-haspopup="dialog" aria-expanded=move || stats_open.get().to_string() on:click=move |_|{stats_open.set(true);focus_stats_close();practice_time_ms.set(None);let promise=load_practice_time();spawn_local(async move{match JsFuture::from(promise).await{Ok(value)=>practice_time_ms.set(value.as_f64()),Err(error)=>import_error.set(Some(format!("Practice time could not load: {error:?}")))}});}><ChartNoAxesCombined size=19/></button>
          <a class="brand" href="#" aria-label="Rudiment"><span class="brand-mark" aria-hidden="true"></span><span>"Rudiment"</span></a>
          <div class="exercise-heading"><span class="eyebrow">"CURRENT EXERCISE"</span><strong>{move || exercise.get().title}</strong><small>"4/4 · sticking and accents"</small></div>
          <button id="settings-trigger" class="stats-trigger settings-trigger" aria-label="Open settings" title="Rudiment settings" aria-haspopup="dialog" aria-expanded=move || settings_open.get().to_string() on:click=move |_|{settings_open.set(true);focus_settings_close();}><SettingsIcon size=19/></button>
        </header>

          <section class=move || if highway.get(){"workspace highway-workspace"}else{"workspace"}>
              <aside class=move || if welcome.get(){"welcome"}else{"welcome hidden"}><div><strong>              "A paradiddle is ready."</strong><span>"Space pauses; R restarts."</span></div><button aria-label="Got it, dismiss welcome hint" on:click=move |_|{welcome.set(false);save_welcome();}>"Got it"</button></aside>
            <section id="score-card" class=move || if highway.get(){"score-card highway-active"}else if notation_dark.get(){"score-card notation-dark"}else{"score-card"}>
              <div class="score-meta">
                <div class="score-view-controls">
                  <button type="button" class=move || if highway.get(){"notation-theme-toggle hidden"}else{"notation-theme-toggle"} aria-label=move || if notation_dark.get(){"Notation theme: Dark. Switch to Light"}else{"Notation theme: Light. Switch to Dark"} aria-pressed=move || notation_dark.get().to_string() title=move || if notation_dark.get(){"Switch to light notation"}else{"Switch to dark notation"} on:click=move |_|{let dark=!notation_dark.get_untracked();notation_dark.set(dark);save_notation_theme(dark);}>{move || if notation_dark.get(){view!{<Moon size=18/>}.into_any()}else{view!{<Sun size=18/>}.into_any()}}</button>
                </div>
                <div class="metronome-context">
                  <button id="visual-metronome" class="visual-metronome" type="button" aria-label=move || format!("Metronome settings, {:.0} BPM", bpm.get()) aria-haspopup="dialog" aria-controls="metronome-popover" aria-expanded=move || (context_popup.get()==Some(ContextPopup::Metronome)).to_string() on:click=move |_|context_popup.update(|popup|*popup=if *popup==Some(ContextPopup::Metronome){None}else{Some(ContextPopup::Metronome)})>
                    <span class="visual-metronome-sweep" aria-hidden="true"><i></i></span>
                    <span class="visual-metronome-beats" aria-hidden="true">{move || (0..beats_per_bar(&exercise.get()).round().max(1.0) as usize).map(|_|view!{<i><span class="visual-metronome-glow"></span><span class="visual-metronome-highlight"></span></i>}).collect_view()}</span>
                    <small class="visual-metronome-label" aria-hidden="true">{move || format!("{:.0} BPM", bpm.get())}</small>
                  </button>
                    <div id="metronome-popover" class=move || if context_popup.get()==Some(ContextPopup::Metronome){"bar-selection-popover metronome-popover"}else{"bar-selection-popover metronome-popover hidden"} role="dialog" aria-label="Metronome">
                      <div class="bar-selection-popover-head"><strong>"Metronome"</strong><button class="bar-selection-close" type="button" aria-label="Close metronome settings" on:click=move |_|{context_popup.set(None);focus_popup_trigger(ContextPopup::Metronome);}><X size=15/></button></div>
                      <MetronomeSettings bpm=bpm count_in=count_in subdivision=subdivision timbre=timbre metronome=metronome click_volume=click_volume snare_volume=snare_volume snare_muted=snare_muted error=metronome_preview_error change_tempo=change_tempo.clone() save=save_current_settings.clone()/>
                    </div>
                </div>
                <details class="score-file-info">
                  <summary role="button" aria-label="File information" title="File information"><Info size=16/></summary>
                  <div class="score-file-info-popover">
                    <strong>{move || if imported_name.get().is_empty(){exercise.get().title}else{imported_name.get()}}</strong>
                    <span>{move || format!("{} playable drum track{}",exercise.get().playable_tracks.len(),if exercise.get().playable_tracks.len()==1{""}else{"s"})}</span>
                    <label class={move || if exercise.get().playable_tracks.len()>1{"track-picker"}else{"track-picker hidden"}}>"Drum track"<select aria-label="Drum track" prop:value=move || exercise.get().selected_track.to_string() on:change=move |event|{let index=event_target_value(&event).parse::<usize>().unwrap_or(0);if let Ok(parsed)=parse_midi(&imported_bytes.get_untracked(),&imported_name.get_untracked(),Some(index)){exercise.set(parsed);}}>{move || exercise.get().playable_tracks.into_iter().map(|track|view!{<option value=track.index.to_string()>{track.name}</option>}).collect_view()}</select></label>
                    <ul class={move || if exercise.get().warnings.is_empty(){"file-warnings hidden"}else{"file-warnings"}}>{move || exercise.get().warnings.into_iter().map(|warning|view!{<li>{warning}</li>}).collect_view()}</ul>
                  </div>
                </details>
              </div>
              <div id="notation" class="notation" role="region" aria-label="Musical notation" aria-live="off" tabindex="0"></div>
              <div id="highway" class=move || if highway.get(){"highway-view"}else{"highway-view hidden"} role="region" aria-label="Drum highway">
                <div id="highway-timeline" class="highway-timeline"></div>
                <p id="highway-error" class="highway-error hidden" role="status"></p>
              </div>
              <div class=move || if import_error.get().is_some(){"error-row"}else{"error-row hidden"}>{move || import_error.get().unwrap_or_default()}</div>
              <div class="position-row">
                <div class=move || if context_popup.get()==Some(ContextPopup::Bars){"bar-selection-control open"}else{"bar-selection-control"}>
                  <button id="bar-selection-trigger" class="bar-selection-trigger" type="button" aria-labelledby="bar-selection-label bar-selection-current" aria-haspopup="dialog" aria-controls="bar-selection-popover" aria-expanded=move || (context_popup.get()==Some(ContextPopup::Bars)).to_string() on:click=move |_|context_popup.update(|popup|*popup=if *popup==Some(ContextPopup::Bars){None}else{Some(ContextPopup::Bars)})><span id="bar-selection-label" class="sr-only">"Select bars"</span><span id="bar-selection-current">{move || {let sequence=bar_sequence.get();let total=sequence.total_bars;let current=displayed_bar.get();if repeat_bar_until_good.get(){format!("BAR {current} / {total} · TRY {}",bar_attempt.get())}else{format!("BAR {current} / {total}")}}}</span></button>
                  <div id="bar-selection-popover" class=move || if context_popup.get()==Some(ContextPopup::Bars){"bar-selection-popover"}else{"bar-selection-popover hidden"} role="dialog" aria-label="Bar selection">
                    <div class="bar-selection-popover-head"><strong>"Practise bars"</strong><button class="bar-selection-close" type="button" aria-label="Close bar selection" on:click=move |_|{context_popup.set(None);focus_popup_trigger(ContextPopup::Bars);}><X size=15/></button></div>
                    <div class="bar-checkboxes" role="group" aria-label="Individual practice bars">{move || {let count=(exercise_beats(&exercise.get())/beats_per_bar(&exercise.get())).ceil().max(1.0) as usize;let toggle=toggle_bar.get_value();(0..count).map(|bar|{let toggle=toggle.clone();view!{<label><input type="checkbox" disabled=move || adaptive_session_active.get() aria-label=format!("Bar {}",bar+1) prop:checked=move || selected_bars.get().binary_search(&bar).is_ok() on:change=move |_|toggle(bar)/><span>{format!("Bar {}",bar+1)}</span></label>}}).collect_view()}}</div>
                    <button class="select-all-bars" type="button" on:click=move |_|{if phase.get_untracked()=="paused" && !selected_bars.get_untracked().is_empty(){selection_changed.set(true);}selected_bars.set(vec![]);save_selection_all();}>"All bars"</button>
                    <small>{move || {let bars=selected_bars.get();if bars.is_empty(){"All bars · Performance when played once".into()}else{format!("{} selected · Practice",bars.iter().map(|bar|format!("{}",bar+1)).collect::<Vec<_>>().join(", "))}}}</small>
                  </div>
                </div>
                <div class="timeline"><i style=move || {let sequence=bar_sequence.get();let fraction=(((timeline_beat.get().max(sequence.origin)-sequence.origin)/sequence.length()*100.0).rem_euclid(100.0).clamp(0.0,100.0))/100.0;format!("transform:scaleX({:.4})",fraction)}></i></div>
                <select class="loop-control" disabled=move || adaptive_session_active.get() aria-label=move || format!("Loops: {}", match loops.get(){2=>"2 LOOPS",4=>"4 LOOPS",0=>"∞ LOOP",_=>"1 / 1 LOOP"}) title="Choose how many times to play the selected bars" prop:value=move || loops.get().to_string() on:change={let save=save_loops.clone();move |event|{let value=event_target_value(&event).parse().unwrap_or(1);if phase.get_untracked()=="paused" && value!=loops.get_untracked(){selection_changed.set(true);}loops.set(value);save();}}><option value="1">"1 / 1 LOOP"</option><option value="2">"2 LOOPS"</option><option value="4">"4 LOOPS"</option><option value="0">"∞ LOOP"</option></select>
              </div>
              <div class=move || if result_modal_open.get(){"result-backdrop"}else{"result-backdrop hidden"} on:click=move |_|dismiss_result(result_modal_open,result_dismiss_after_ms,historical_result,stats_open)>
              <section id="result-dialog" class="stats-modal results-modal" role="dialog" aria-modal="true" aria-labelledby="result-title" on:click=|event|event.stop_propagation() on:keydown=move |event:web_sys::KeyboardEvent|{
                  match event.key().as_str(){
                    "Escape"=>{event.prevent_default();event.stop_propagation();dismiss_result(result_modal_open,result_dismiss_after_ms,historical_result,stats_open);},
                    "Tab"=>trap_dialog_tab(&event,"#result-dialog"),
                    _=>{},
                  }
                }>
                <header class="summary-header">
                  <div class="summary-title-group"><span class="summary-icon" aria-hidden="true">"R"</span><div><h2 id="result-title">"Session Complete"</h2><p>{move || displayed_result.get().map(|session|format!("{} · {:.0} BPM",session.exercise_title,session.bpm)).unwrap_or_default()}</p></div></div>
                  <button id="result-modal-close" aria-label="Close session results" on:click=move |_|dismiss_result(result_modal_open,result_dismiss_after_ms,historical_result,stats_open)><X size=21/></button>
                </header>
                <div class="summary-content">
                <div class="summary-cards">
                  <section class="summary-card score-summary-card summary-achievement">
                    <h3>"Session Score"</h3>
                    <div class="summary-score"><strong>{move || display_score(displayed_result.get().and_then(|session|session.summary.score))}</strong><span>"/ 100"</span></div>
                    <div class=move || if displayed_result.get().is_some_and(|session|is_white_mastery(&session.summary)){"summary-stars mastery"}else{"summary-stars"} aria-label=move || displayed_result.get().map(|session|{let rating=performance_rating(&session.summary);if is_white_mastery(&session.summary){"Mastery, three white stars".into()}else{format!("{} of 3 gold stars",rating.stars)}}).unwrap_or_default()>{move || {let rating=displayed_result.get().map(|session|performance_rating(&session.summary)).unwrap_or(PerformanceRating{stars:0,award:PerformanceAward::None});(0..3).map(|index|view!{<Star size=38 fill=if index<rating.stars{"currentColor"}else{"none"}/>}).collect_view()}}</div>
                    <strong class="summary-classification">{move || displayed_result.get().map(|session|achievement_copy(&session.summary).0).unwrap_or("")}</strong>
                    <p>{move || displayed_result.get().map(|session|achievement_copy(&session.summary).1).unwrap_or("")}</p>
                    <span class=move || if historical_result.get().is_none()&&displayed_result.get().is_some_and(|session|is_new_high_score(&session,&history.get())){"new-high-score"}else{"new-high-score hidden"}>"↗ New high score"</span>
                  </section>
                  <section class="summary-card breakdown-card">
                    <h3>"Hit Breakdown"</h3>
                    <div class="breakdown-list">{move || displayed_result.get().map(|session|{let (perfect,good,early,late)=timing_hit_counts(&session.summary);[("Perfect","perfect",perfect),("Good","good",good),("Early","early",early),("Late","late",late),("Missed","missed",session.summary.missed)].into_iter().map(|(label,class,count)|{let value=percentage(count,session.summary.expected);view!{<article class=format!("breakdown-row {class}")><span><i></i>{label}</span><strong>{format!("{count} / {}",session.summary.expected)}</strong><em><i style=format!("width:{value:.3}%")></i></em><b>{format!("{value:.0}%")}</b></article>}}).collect_view()}).unwrap_or_default()}</div>
                  </section>
                </div>
                <section class="summary-card progress-card">
                  <h3>"Your Progress"</h3><p>"Session score over time"</p>
                  {move || displayed_result.get().map(|session|{let entries=progress_sessions(&session,&history.get());let scores=entries.iter().map(|(_,score)|*score).collect::<Vec<_>>();let points=progress_chart_points(&scores);let selected=entries.iter().position(|(id,_)|id==&session.id);let line=points.iter().map(|(x,y)|format!("{x:.1},{y:.1}")).collect::<Vec<_>>().join(" ");let latest=selected.and_then(|index|points.get(index).copied().zip(scores.get(index).copied()));view!{
                    <svg class="progress-chart" viewBox="0 0 760 190" preserveAspectRatio="none" role="img" aria-label=format!("Scores for {} sessions through the selected session",scores.len())>
                      <title>{format!("{} scores for this exercise through the selected session",scores.len())}</title>
                      {[100_u8,75,50,25,0].into_iter().map(|value|{let y=146.0-132.0*value as f64/100.0;view!{<g class="progress-grid"><line x1="48" y1=y.to_string() x2="712" y2=y.to_string()/><text x="36" y=(y+4.0).to_string()>{value}</text></g>}}).collect_view()}
                      <polyline class="progress-line" points=line/>
                      {points.iter().enumerate().map(|(index,(x,y))|view!{<g><circle class=if Some(index)==selected{"progress-point latest"}else{"progress-point"} data-session-id=entries[index].0.clone() data-score=entries[index].1.to_string() cx=x.to_string() cy=y.to_string() r=if Some(index)==selected{"5"}else{"3"}/><text class="progress-session-label" x=x.to_string() y="170">{index+1}</text></g>}).collect_view()}
                      {latest.map(|((x,y),score)|{let label_x=(x-22.0).clamp(4.0,712.0);let label_y=(y-31.0).max(1.0);view!{<g class="progress-latest-label"><rect x=label_x.to_string() y=label_y.to_string() width="44" height="22" rx="6"/><text x=(label_x+22.0).to_string() y=(label_y+15.0).to_string()>{format!("{score:.0}")}</text></g>}})}
                      <text class="progress-axis-label" x="380" y="188">"Session"</text>
                    </svg>
                  }.into_any()}).unwrap_or_else(||().into_any())}
                </section>
                </div>
                <footer class="summary-footer"><p>{move || displayed_result.get().map(|session|format!("{} extra · {} wrong drum{}",session.summary.extras,session.summary.wrong_instrument,if session.summary.wrong_instrument==1{""}else{"s"})).unwrap_or_default()}</p><div class="result-actions"><button on:click=move |_|dismiss_result(result_modal_open,result_dismiss_after_ms,historical_result,stats_open)>{move || if historical_result.get().is_some(){"Back to history"}else{"Back to score"}}</button><button class=move || if historical_result.get().is_some(){"primary hidden"}else{"primary"} on:click={let start=start_new_run.clone();move |_|{result_modal_open.set(false);result_dismiss_after_ms.set(f64::INFINITY);observed.set(vec![]);live.set(vec![]);result.set(None);progress.set(-1.0);timeline_beat.set(-1.0);start(count_in.get_untracked());}}>"Perform again"</button></div></footer>
                </section>
              </div>
            </section>
            <section class="practice-controls">
              <div class="transport">
                <button class="restart-button" on:click={let reset=reset_run.clone();move |_|reset()}><span class="transport-action-label"><span class="transport-icon" aria-hidden="true"><RotateCcw size=24/></span><span>"Restart"</span></span><small>"Clear current run"</small></button>
                <button class="primary start-button" aria-label=move || match phase.get().as_str(){"running"|"count_in"|"starting"=>String::from("Pause"),"paused" if selection_changed.get()=>String::from("Start selected bars"),"paused"=>String::from("Resume"),_=>{let sequence=bar_sequence.get();if sequence.bars.len()==sequence.total_bars&&loops.get()==1{String::from("Start performance Hit your drum or press J to start Start practice")}else{String::from("Start practice Hit your drum or press J to start Start performance")}}} on:click={let start=start_new_run.clone();let pause=button_pause.clone();let resume=button_resume.clone();move |_|match phase.get_untracked().as_str(){"running"|"count_in"|"starting"=>pause(),"paused"=>resume(),_=>{observed.set(vec![]);live.set(vec![]);result.set(None);input_mode.set(if selected_midi.get_untracked().is_some(){InputSource::Midi}else{InputSource::Demo});start(count_in.get_untracked());}}}><span class="transport-action-label"><span class="transport-icon" aria-hidden="true">{move || if matches!(phase.get().as_str(),"running"|"count_in"|"starting"){view!{<Pause size=21/>}.into_any()}else{view!{<Play size=21 fill="currentColor"/>}.into_any()}}</span><span>{move || match phase.get().as_str(){"running"|"count_in"|"starting"=>"Pause","paused" if selection_changed.get()=>"Start selected bars","paused"=>"Resume",_=>{let sequence=bar_sequence.get();if sequence.bars.len()==sequence.total_bars&&loops.get()==1{"Start performance "}else{"Start practice "}}}}</span></span><small class=move || if phase.get()=="idle"{"start-instruction"}else{"start-instruction hidden"}>"Hit your drum or press J to start"</small></button>
                <aside class="timing-key" aria-label="Timing feedback key">
                  <span class="perfect"><i aria-hidden="true"></i>"Perfect"</span>
                  <span class="good"><i aria-hidden="true"></i>"Good"</span>
                  <span class="early"><i aria-hidden="true"></i>"Early"</span>
                  <span class="late"><i aria-hidden="true"></i>"Late"</span>
                  <span class="unmatched"><i aria-hidden="true"></i>"Unmatched"</span>
                </aside>
              </div>
            </section>
          </section>

            <div class=move || if library_open.get(){"stats-backdrop library-backdrop"}else{"stats-backdrop library-backdrop hidden"} on:click=move |_|{pending_import_delete.set(None);library_open.set(false);restore_library_focus();}>
              <section id="midi-library-dialog" class="stats-modal library-modal" role="dialog" aria-modal="true" aria-labelledby="midi-library-title" inert=move || pending_import_delete.get().is_some() on:click=|event|event.stop_propagation() on:keydown=move |event:web_sys::KeyboardEvent|{
                match event.key().as_str(){
                  "Escape"=>{event.prevent_default();event.stop_propagation();pending_import_delete.set(None);library_open.set(false);restore_library_focus();},
                  "Tab"=>trap_dialog_tab(&event,"#midi-library-dialog"),
                  "j"|"J"|"r"|"R"=>{event.prevent_default();event.stop_propagation();},
                  " " if !keyboard_targets_control(&event)=>{event.prevent_default();event.stop_propagation();},
                  _=>{},
                }
              }>
                <div class="stats-modal-header library-modal-header">
                  <div><span class="eyebrow">"EXERCISES"</span><h2 id="midi-library-title">"MIDI library"</h2><p>{move || format!("{} built-in · {} personal",BUILTIN_RUDIMENTS.len(),saved_imports.get().len())}</p></div>
                  <button id="midi-library-close" aria-label="Close MIDI library" on:click=move |_|{pending_import_delete.set(None);library_open.set(false);restore_library_focus();}><X size=20/></button>
                </div>
                <div class="library-actions">
                  <button class="primary" on:click={let callback=choose_cb.clone();let stop=stop_for_import.clone();move |_|{
                    stop();
                    choose_midi(callback.as_ref().as_ref().unchecked_ref(), MAX_MIDI_FILE_BYTES);
                  }}>"Import MIDI"</button>
                </div>
                <div class="library-tabs" role="tablist" aria-label="MIDI library sections">
                  <button role="tab" aria-selected=move || (!library_personal.get()).to_string() class=move || if library_personal.get(){""}else{"active"} on:click=move |_|{library_personal.set(false);library_page.set(0);}>{format!("Built-in · {}",BUILTIN_RUDIMENTS.len())}</button>
                  <button role="tab" aria-selected=move || library_personal.get().to_string() class=move || if library_personal.get(){"active"}else{""} on:click=move |_|{library_personal.set(true);library_page.set(0);}>{move || format!("Your imports · {}",saved_imports.get().len())}</button>
                </div>
                <label class=move || if library_personal.get(){"library-difficulty hidden"}else{"library-difficulty"}>"Difficulty"<select aria-label="Exercise difficulty" prop:value=move || library_difficulty.get().map(Difficulty::label).unwrap_or("All") on:change=move |event|{let value=event_target_value(&event);library_difficulty.set(parse_difficulty_label(value.as_str()));library_page.set(0);}><option value="All">"All levels"</option><option value="Beginner">"Beginner"</option><option value="Intermediate">"Intermediate"</option><option value="Advanced">"Advanced"</option></select></label>
                <div class="library-page" role="list">
                  {move || if library_personal.get(){
                    let imports=saved_imports.get();
                    let pages=imports.len().div_ceil(LIBRARY_PAGE_SIZE).max(1);
                    let page=library_page.get().min(pages-1);
                    if imports.is_empty(){
                      view!{<div class="library-empty-state"><strong>"No personal imports yet"</strong><span>"Import a MIDI file to add it to your library on this device."</span></div>}.into_any()
                    }else{
                      imports.into_iter().skip(page*LIBRARY_PAGE_SIZE).take(LIBRARY_PAGE_SIZE).map(|item|{
                        let hash=item.hash.clone();
                        let name=item.name.clone();
                        let duplicate_name=saved_imports.get_untracked().iter().filter(|saved|saved.name==name).count()>1;
                        let display_name=if duplicate_name{format!("{} · {}",name,&hash[..6])}else{name.clone()};
                        let class_hash=hash.clone();
                        let load_hash=hash.clone();
                        let delete_hash=hash.clone();
                        let delete_name=name.clone();
                        view!{
                          <article class="library-card personal-library-card" role="listitem">
                            <button class=move || if selected_import.get().as_deref()==Some(class_hash.as_str()){"library-load selected"}else{"library-load"} aria-current=move || if selected_import.get().as_deref()==Some(hash.as_str()){"true"}else{"false"} on:click=move |_|{
                              library_open.set(false);
                              restore_library_focus();
                              let hash=load_hash.clone();
                              spawn_local(async move{
                                if let Err(error)=JsFuture::from(load_saved_import(&hash)).await{
                                  import_error.set(Some(format!("Saved MIDI file could not be loaded: {}",error.as_string().unwrap_or_else(||format!("{error:?}")))));
                                }
                              });
                            }><strong>{display_name}</strong><small>"Saved on this device"</small></button>
                            <button id=format!("delete-import-{delete_hash}") class="library-delete" aria-label=format!("Delete {delete_name}") on:click=move |event:web_sys::MouseEvent|{
                              event.stop_propagation();
                              pending_import_delete.set(Some((delete_hash.clone(),delete_name.clone())));
                              focus_import_delete_cancel();
                            }>"Delete"</button>
                          </article>
                        }
                      }).collect_view().into_any()
                    }
                  }else{
                    let difficulty=library_difficulty.get();
                    let page=library_page.get().min(builtin_count(difficulty).div_ceil(LIBRARY_PAGE_SIZE).max(1)-1);
                    BUILTIN_RUDIMENTS.into_iter().filter(|item|difficulty.is_none_or(|level|item.difficulty==level)).skip(page*LIBRARY_PAGE_SIZE).take(LIBRARY_PAGE_SIZE).map(|item|{
                      let midi_url=format!("public/exercises/rudiments/{}.mid",item.id);
                      let load_name=format!("{}.mid",item.title);
                      view!{
                        <article class="library-card bundled-entry" role="listitem">
                          <button class="library-load" on:click=move |_|{
                            library_open.set(false);
                            restore_library_focus();
                            load_bundled_from_library(&midi_url,&load_name);
                          }><strong>{item.title}</strong><small>{format!("{} · {}",item.difficulty.label(),item.description)}</small></button>
                        </article>
                      }
                    }).collect_view().into_any()
                  }}
                </div>
                <div class="library-pagination" aria-label="Library pagination">
                  <button aria-label="Previous library page" disabled=move || library_page.get()==0 on:click=move |_|library_page.update(|page|*page=page.saturating_sub(1))><ArrowLeft size=13/>"Previous"</button>
                  <span>{move || {let count=if library_personal.get(){saved_imports.get().len()}else{builtin_count(library_difficulty.get())};let pages=count.div_ceil(LIBRARY_PAGE_SIZE).max(1);let page=library_page.get().min(pages-1);format!("Page {} of {}",page+1,pages)}}</span>
                  <button aria-label="Next library page" disabled=move || {let count=if library_personal.get(){saved_imports.get().len()}else{builtin_count(library_difficulty.get())};library_page.get()+1>=count.div_ceil(LIBRARY_PAGE_SIZE).max(1)} on:click=move |_|{let count=if library_personal.get_untracked(){saved_imports.get_untracked().len()}else{builtin_count(library_difficulty.get_untracked())};let pages=count.div_ceil(LIBRARY_PAGE_SIZE).max(1);library_page.update(|page|*page=(*page+1).min(pages-1));}>"Next"<ArrowRight size=13/></button>
                </div>
              </section>
              {move || pending_import_delete.get().map(|(hash,name)|{
                let backdrop_hash=hash.clone();
                let cancel_hash=hash.clone();
                let confirm_hash=hash.clone();
                view!{
                  <div class="delete-confirm-backdrop" on:click=move |event:web_sys::MouseEvent|{event.stop_propagation();dismiss_import_delete(pending_import_delete,backdrop_hash.clone());}>
                    <section id="import-delete-dialog" class="delete-confirm-dialog" role="alertdialog" aria-modal="true" aria-labelledby="import-delete-title" aria-describedby="import-delete-description" on:click=|event|event.stop_propagation() on:keydown=move |event:web_sys::KeyboardEvent|{
                      if event.key()=="Tab"{
                        trap_dialog_tab(&event,"#import-delete-dialog");
                        event.stop_propagation();
                      }
                    }>
                      <span class="eyebrow">"REMOVE IMPORT"</span>
                      <h2 id="import-delete-title">"Delete MIDI file?"</h2>
                      <p id="import-delete-description">{format!("“{name}” will be permanently removed from the MIDI library on this browser.")}</p>
                      <div class="delete-confirm-actions">
                        <button id="import-delete-cancel" on:click=move |event:web_sys::MouseEvent|{event.stop_propagation();dismiss_import_delete(pending_import_delete,cancel_hash.clone());}>"Cancel"</button>
                        <button class="delete-confirm-danger" on:click=move |event:web_sys::MouseEvent|{
                          event.stop_propagation();
                          pending_import_delete.set(None);
                          focus_library_close();
                          let hash=confirm_hash.clone();
                          spawn_local(async move{
                            match JsFuture::from(delete_import(&hash)).await{
                              Ok(_)=>{
                                saved_imports.update(|items|items.retain(|item|item.hash!=hash));
                                if selected_import.get_untracked().as_deref()==Some(hash.as_str()){
                                  selected_import.set(None);
                                  if let Err(error)=JsFuture::from(save_last_import(JsValue::NULL)).await{
                                    import_error.set(Some(format!("File deleted, but the last-selection setting could not be updated: {}",error.as_string().unwrap_or_else(||format!("{error:?}")))));
                                  }
                                }
                              }
                              Err(error)=>import_error.set(Some(format!("Could not delete saved MIDI file: {}",error.as_string().unwrap_or_else(||format!("{error:?}"))))),
                            }
                          });
                        }>"Delete file"</button>
                      </div>
                    </section>
                  </div>
                }
              })}
            </div>

        {move || if stats_open.get(){
          view!{
            <div class="stats-backdrop" on:click=move |_|{stats_open.set(false);restore_stats_focus();}>
              <section id="stats-dialog" class="stats-modal" role="dialog" aria-modal="true" aria-labelledby="stats-title" on:click=|event|event.stop_propagation() on:keydown=move |event:web_sys::KeyboardEvent|{
                match event.key().as_str(){
                  "Escape"=>{event.prevent_default();stats_open.set(false);restore_stats_focus();},
                  "Tab"=>trap_dialog_tab(&event,"#stats-dialog"),
                  _=>{},
                }
              }>
                <div class="stats-modal-header">
                  <div><span class="eyebrow">"YOUR PERFORMANCES"</span><h2 id="stats-title">"Performance history"</h2></div>
                  <button id="stats-close" autofocus=true aria-label="Close statistics" on:click=move |_|{stats_open.set(false);restore_stats_focus();}><X size=20/></button>
                </div>
                <div class="stats-overview">
                  <article><span>"SESSIONS"</span><strong>{move || history.get().len()}</strong></article>
                  <article><span>"PERSONAL BEST"</span><strong>{move || display_score(history.get().iter().filter_map(|session|session.summary.score).max_by(|left,right|left.total_cmp(right)))}</strong></article>
                  <article><span>"AVERAGE SCORE"</span><strong>{move || {let scores=history.get().iter().filter_map(|session|session.summary.score).collect::<Vec<_>>();if scores.is_empty(){"—".into()}else{format!("{:.1}%",scores.iter().sum::<f64>()/scores.len() as f64)}}}</strong></article>
                  <article><span>"PERFECT HITS"</span><strong>{move || history.get().iter().map(|session|timing_hit_counts(&session.summary).0).sum::<usize>()}</strong></article>
                  <article><span>"GOOD HITS"</span><strong>{move || history.get().iter().map(|session|timing_hit_counts(&session.summary).1).sum::<usize>()}</strong></article>
                  <article><span>"EARLY HITS"</span><strong>{move || history.get().iter().map(|session|timing_hit_counts(&session.summary).2).sum::<usize>()}</strong></article>
                  <article><span>"LATE HITS"</span><strong>{move || history.get().iter().map(|session|timing_hit_counts(&session.summary).3).sum::<usize>()}</strong></article>
                  <article><span>"MISSED NOTES"</span><strong>{move || history.get().iter().map(|session|session.summary.missed).sum::<usize>()}</strong></article>
                  <article class="practice-time-card"><span>"PRACTICE TIME"</span><strong>{move || practice_time_ms.get().map(format_practice_time).unwrap_or_else(||"—".into())}</strong></article>
                </div>
                <p class="practice-time-help">"Practice time is recorded on this device during playback; older sessions have no recorded duration."</p>
                <p class=move || if incomplete_history.get()!=0{"practice-time-help"}else{"practice-time-help hidden"} role="status">{move || format!("{} incomplete historical record(s) could not be displayed.",incomplete_history.get())}</p>
                <h3>"Recent sessions"</h3>
                <ol class="stats-recent">
                  {move || if history.get().is_empty(){
                    view!{<li class="stats-empty">"Complete a performance to see your statistics here."</li>}.into_any()
                  }else{
                    history.get().iter().rev().take(8).map(|session|{let selected=session.clone();view!{
                      <li>
                        <button class="stats-session" on:click=move |_|{historical_result.set(Some(selected.clone()));stats_open.set(false);result_dismiss_after_ms.set(f64::INFINITY);result_modal_open.set(true);focus_result_close();}>
                        <span><strong>{session.exercise_title.clone()}</strong><small>{let (perfect,good,early,late)=timing_hit_counts(&session.summary);format!("{:.0} BPM · {} · {} · {} perfect, {} good, {} early, {} late, {} missed",session.bpm,match session.input_mode{InputSource::Demo=>"Demo",InputSource::Midi=>"MIDI"},session.ended_at.get(..10).unwrap_or_default(),perfect,good,early,late,session.summary.missed)}</small></span>
                        <b>{format!("{}/100",display_score(session.summary.score))}</b>
                        </button>
                      </li>
                    }}).collect_view().into_any()
                  }}
                </ol>
              </section>
            </div>
          }.into_any()
        }else{().into_any()}}

        <aside id="settings-dialog" class=move || if settings_open.get(){"settings-drawer open"}else{"settings-drawer"} role="dialog" aria-modal="true" aria-labelledby="settings-title" aria-hidden=move || (!settings_open.get()).to_string() inert=move || !settings_open.get() on:keydown=move |event:web_sys::KeyboardEvent|{
          match event.key().as_str(){
            "Escape"=>{event.prevent_default();settings_open.set(false);restore_settings_focus();},
            "Tab"=>trap_dialog_tab(&event,"#settings-dialog"),
            _=>{},
          }
        }>
          <div class="drawer-head"><div><span class="eyebrow">"RUDIMENT SETUP"</span><h2 id="settings-title">"Sound & drums"</h2></div><button id="settings-close" aria-label="Close settings" on:click=move |_|{settings_open.set(false);restore_settings_focus();}><X size=22/></button></div>
          <div class="drawer-scroll">
            <section><h3>"Practice display"</h3>
              <p class="help">"Choose how the exercise is shown: the written drum notation or a scrolling drum highway."</p>
              <div class="visualisation-selector" role="group" aria-label="Musical visualisation">
                <button type="button" class=move || if !highway.get(){"active"}else{""} aria-pressed=move || (!highway.get()).to_string() on:click=move |_|{highway.set(false);save_highway_mode(false);}>"Notation"</button>
                <button type="button" class=move || if highway.get(){"active"}else{""} aria-pressed=move || highway.get().to_string() on:click=move |_|{highway.set(true);save_highway_mode(true);}>"Highway"</button>
              </div>
              <div class="button-row"><button on:click=move |_|{settings_open.set(false);onboarding_open.set(true);}>"Reopen setup page…"</button></div>
            </section>
            <section><h3>"Metronome"</h3>
              <MetronomeSettings bpm=bpm count_in=count_in subdivision=subdivision timbre=timbre metronome=metronome click_volume=click_volume snare_volume=snare_volume snare_muted=snare_muted error=metronome_preview_error change_tempo=change_tempo.clone() save=save_current_settings.clone()/>
            </section>
            <section><h3>"Session statistics"</h3><label class="toggle-row"><span>"Automatically show session statistics"<small>"Results are always saved after a performance."</small></span><input type="checkbox" prop:checked=move || auto_show_statistics.get() on:change=move |event|{auto_show_statistics.set(event_target_checked(&event));save_auto_statistics();}/></label></section>
            <section><h3>"Reference drums"</h3><label class="toggle-row"><span>"Play app drums"<small>"Mute this if your drum module is audible."</small></span><input type="checkbox" prop:checked=move || !snare_muted.get() on:change=move |event|{snare_muted.set(!event_target_checked(&event));set_volumes(if snare_muted.get_untracked(){0.0}else{snare_volume.get_untracked()},click_volume.get_untracked());save_snare_mute();}/></label><label>"Play drums"<select aria-label="Reference drum timing" prop:value=move || snare_reference_mode.get() on:change=move |event|{let value=event_target_value(&event);snare_reference_mode.set(if value=="notes"{"notes".into()}else{"hits".into()});save_snare_reference_mode();}><option value="hits">"With your hits"</option><option value="notes">"On written notes"</option></select><small class="help">{move || if snare_reference_mode.get()=="notes"{"Plays each written drum note as a reference; your strikes are still scored silently."}else{"Plays immediately when Rudiment receives your keyboard or MIDI strike."}}</small></label><label>"Drum volume"<input type="range" min="0" max="1" step="0.01" prop:value=move || snare_volume.get() on:input=move |event|{let value=event_target_value(&event).parse().unwrap_or(0.72);snare_volume.set(value);set_volumes(if snare_muted.get_untracked(){0.0}else{value},click_volume.get_untracked());save_snare_volume();}/></label></section>
            <section><h3>"Adaptive learning"</h3>
              <label class="toggle-row"><span>"Enable adaptive learning"<small>"When enabled, practice starts at the original tempo and adapts automatically. Disables repeat-bar mode."</small></span><input type="checkbox" aria-label="Enable adaptive learning" prop:checked=move || adaptive_enabled.get() on:change=move |event|{
                let enabled = event_target_checked(&event);
                if enabled {
                  repeat_bar_until_good.set(false);
                  adaptive_enabled.set(true);
                  adaptive_original_bpm.set(bpm.get_untracked());
                  adaptive_original_bars.set(selected_bars.get_untracked());
                  adaptive_original_loops.set(loops.get_untracked());
                } else {
                  if adaptive_session_active.get_untracked() {
                    set_practice_active(false); stop_audio(); phase.set("idle".into());
                    adaptive_session_active.set(false); adaptive_bars.set(None); adaptive_status.set(None);
                    bpm.set(adaptive_original_bpm.get_untracked()); loops.set(adaptive_original_loops.get_untracked());
                  }
                  adaptive_enabled.set(false);
                }
                save_current_settings();
              }/></label>
              <label>{move || format!("Initial recovery tempo · {:.0}%", adaptive_settings.get().initial_recovery_percent)}<input type="range" min="30" max="90" step="1" prop:value=move || adaptive_settings.get().initial_recovery_percent on:input=move |event|{let v=event_target_value(&event).parse().unwrap_or(50.0); adaptive_settings.update(|s| s.initial_recovery_percent=v); save_adaptive_settings();}/></label>
              <label>{move || format!("Minimum BPM · {:.0}", adaptive_settings.get().min_bpm)}<input type="range" min="30" max="100" step="1" prop:value=move || adaptive_settings.get().min_bpm on:input=move |event|{let v=event_target_value(&event).parse().unwrap_or(50.0); adaptive_settings.update(|s| s.min_bpm=v); save_adaptive_settings();}/></label>
              <label>{move || format!("Tempo increase · {:.0} BPM", adaptive_settings.get().tempo_increase_bpm)}<input type="range" min="1" max="20" step="1" prop:value=move || adaptive_settings.get().tempo_increase_bpm on:input=move |event|{let v=event_target_value(&event).parse().unwrap_or(5.0); adaptive_settings.update(|s| s.tempo_increase_bpm=v); save_adaptive_settings();}/></label>
              <label>{move || format!("Secondary reduction · {:.0}%", adaptive_settings.get().secondary_reduction_percent)}<input type="range" min="5" max="30" step="1" prop:value=move || adaptive_settings.get().secondary_reduction_percent on:input=move |event|{let v=event_target_value(&event).parse().unwrap_or(10.0); adaptive_settings.update(|s| s.secondary_reduction_percent=v); save_adaptive_settings();}/></label>
              <label>{move || format!("Failed attempts before slowing · {}", adaptive_settings.get().failed_attempts_before_slow)}<input type="range" min="1" max="5" step="1" prop:value=move || adaptive_settings.get().failed_attempts_before_slow on:input=move |event|{let v=event_target_value(&event).parse().unwrap_or(2); adaptive_settings.update(|s| s.failed_attempts_before_slow=v); save_adaptive_settings();}/></label>
              <label>{move || format!("Successful repetitions required · {}", adaptive_settings.get().successful_repetitions_required)}<input type="range" min="1" max="5" step="1" prop:value=move || adaptive_settings.get().successful_repetitions_required on:input=move |event|{let v=event_target_value(&event).parse().unwrap_or(2); adaptive_settings.update(|s| s.successful_repetitions_required=v); save_adaptive_settings();}/></label>
              <label>{move || format!("Neighbouring bars each side · {}", adaptive_settings.get().neighbour_bars)}<input type="range" min="0" max="2" step="1" prop:value=move || adaptive_settings.get().neighbour_bars on:input=move |event|{let v=event_target_value(&event).parse().unwrap_or(1); adaptive_settings.update(|s| s.neighbour_bars=v); save_adaptive_settings();}/></label>
              <label class="toggle-row"><span>"Periodic full-speed checks"<small>"Perform one target-speed attempt after the configured number of tempo increases."</small></span><input type="checkbox" prop:checked=move || adaptive_settings.get().full_speed_checks on:change=move |event|{let v=event_target_checked(&event); adaptive_settings.update(|s| s.full_speed_checks=v); save_adaptive_settings();}/></label>
              <label>{move || format!("Full-speed check interval · {} increases", adaptive_settings.get().full_speed_check_interval)}<input type="range" min="1" max="10" step="1" prop:value=move || adaptive_settings.get().full_speed_check_interval on:input=move |event|{let v=event_target_value(&event).parse().unwrap_or(4); adaptive_settings.update(|s| s.full_speed_check_interval=v); save_adaptive_settings();}/></label>
              <button on:click=move |_|{adaptive_settings.set(AdaptiveSettings::default()); save_adaptive_settings();}>"Restore defaults"</button>
            </section>
            <section><h3>"Practice repeats"</h3>
              <label class="toggle-row"><span>"Repeat until successful"<small>"A failed bar is tried again; every failed attempt still reduces the final performance score."</small></span><input aria-label="Repeat bar until Good" type="checkbox" disabled=move || adaptive_enabled.get() prop:checked=move || repeat_bar_until_good.get() on:change={let save=save_repeat_bar.clone();move |event|{repeat_bar_until_good.set(event_target_checked(&event));save();}}/></label>
              <label>"Required accuracy"<select aria-label="Repeat tolerance" prop:value=move || match repeat_tolerance.get(){RepeatTolerance::Relaxed=>"relaxed",RepeatTolerance::Standard=>"standard",RepeatTolerance::Strict=>"strict"} on:change=move |event|{repeat_tolerance.set(match event_target_value(&event).as_str(){"relaxed"=>RepeatTolerance::Relaxed,"strict"=>RepeatTolerance::Strict,_=>RepeatTolerance::Standard});save_repeat_tolerance();}><option value="relaxed">"Relaxed · no misses"</option><option value="standard">"Standard · Good or Perfect"</option><option value="strict">"Strict · Perfect only"</option></select><small class="help">"Extra hits and wrong drums always fail the current attempt. This setting does not change the timing windows."</small></label>
            </section>
            <section><h3>"MIDI drums"</h3><p class="help">"Rudiment connects automatically to your last-used kit. Rudiment never uploads your events."</p>
              <MidiConfigSection midi_status=midi_status midi_ports=midi_ports selected_midi=selected_midi midi_learn=midi_learn midi_bindings=midi_bindings diagnostics=diagnostics midi_modal_open=midi_modal_open connect=connect_action.clone()/>
              <div class="button-row"><button aria-label="Open MIDI connection setup" on:click=move |_|{midi_modal_open.set(true);focus_midi_modal_close();}>"MIDI connection setup…"</button></div>
            </section>
            <section><h3>"Latency calibration"</h3><p class="help">"Tap your snare or J with each of eight clicks. This estimates input offset; it cannot remove all external hardware latency."</p><strong class="cal-value">{move || format!("{:+.0} ms",latency_ms.get())}</strong><div class="button-row"><button on:click=move |_|{calibration_errors.set(vec![]);let promise=start_calibration();spawn_local(async move{if let Ok(value)=JsFuture::from(promise).await{let array=Array::from(&value);calibration_targets.set(array.iter().filter_map(|item|item.as_f64()).collect());}});}>"Start 8-tap calibration"</button><button on:click=move |_|{latency_ms.set(0.0);calibration_targets.set(vec![]);save_cal_reset();}>"Reset"</button></div><label>"Manual offset"<input type="range" min="-200" max="200" step="1" prop:value=move || latency_ms.get() on:input=move |event|{latency_ms.set(event_target_value(&event).parse().unwrap_or(0.0));save_cal_manual();}/></label></section>
            <section><h3>"Timing feedback"</h3><p class="help">{format!("Perfect hits are green (±{DEFAULT_PERFECT_MS:.0} ms by default), Good hits light green, Early hits orange and Late hits purple. Unplayed notes remain neutral. Matching closes outside ±{DEFAULT_MATCH_MS:.0} ms.")}</p><label>{move || format!("Perfect window · ±{:.0} ms",excellent_ms.get())}<input type="range" min="5" max="40" step="1" prop:value=move || excellent_ms.get() on:input=move |event|{let value=event_target_value(&event).parse::<f64>().unwrap_or(DEFAULT_PERFECT_MS).min(on_time_ms.get_untracked());excellent_ms.set(value);save_excellent();}/></label><label>{move || format!("Good window · ±{:.0} ms",on_time_ms.get())}<input type="range" min="20" max="60" step="1" prop:value=move || on_time_ms.get() on:input=move |event|{let value=event_target_value(&event).parse::<f64>().unwrap_or(DEFAULT_GOOD_MS);on_time_ms.set(value);if excellent_ms.get_untracked()>value{excellent_ms.set(value);}save_on_time();}/></label></section>
            <section><h3>"Local data"</h3><p class="help">{move || format!("{} completed performance(s) stored only in this browser.",history.get().len())}</p><div class="button-row"><button on:click=move |_|{let promise=export_history();spawn_local(async move{let _=JsFuture::from(promise).await;});}>"Export JSON"</button><button class="danger" on:click=move |_|{pending_clear_history.set(true);}>"Clear history"</button></div><button class="wide-button" on:click=move |_|{pending_reset_settings.set(true);}>"Reset settings & mappings"</button></section>
            <section id="diagnostics-settings"><h3>"Playhead diagnostics"</h3>
              <p class="help">{format!("Session-only diagnostic switches for investigating playhead smoothness. They are never saved with your settings, and reloading the page restores normal operation. The ?playhead-debug=1, visual-hits and metronome-animation URL parameters still work and these toggles reflect them. Bridge build: {}.", bridge_build())}</p>
              <label class="toggle-row"><span>"Reference playhead overlay + trace"<small>"Blue wall-clock comparison line. It adds a moving overlay of its own, so enable Performance counters alone when you want undisturbed measurements. Export the full trace from a tethered console with rudiment.exportPlayheadDebugCsv()."</small></span><input type="checkbox" aria-label="Reference playhead overlay" prop:checked=move || diagnostic_reference_overlay.get() on:change=move |event|{let value=event_target_checked(&event);diagnostic_reference_overlay.set(value);set_diagnostic("referenceOverlay",value);}/></label>
              <label class="toggle-row"><span>"Visual hit feedback"<small>"Notehead colours, accuracy halos and unmatched-strike markers. Scoring itself is unaffected."</small></span><input type="checkbox" aria-label="Visual hit feedback" prop:checked=move || diagnostic_visual_feedback.get() on:change=move |event|{let value=event_target_checked(&event);diagnostic_visual_feedback.set(value);set_diagnostic("visualFeedback",value);}/></label>
              <label class="toggle-row"><span>"Metronome beat lights"<small>"Beat labels and the current-dot highlight. The sweep needle and dot pulses below independently enable their own elements, even with this switch off."</small></span><input type="checkbox" aria-label="Metronome beat lights" prop:checked=move || diagnostic_metronome_visual.get() on:change=move |event|{let value=event_target_checked(&event);diagnostic_metronome_visual.set(value);set_diagnostic("metronomeVisual",value);}/></label>
              <label class="toggle-row"><span>"Sweep needle"<small>"Independently enables the pendulum sweep - use alone to test the needle in the beat-judder bisection."</small></span><input type="checkbox" aria-label="Metronome sweep needle" prop:checked=move || diagnostic_metronome_needle.get() on:change=move |event|{let value=event_target_checked(&event);diagnostic_metronome_needle.set(value);set_diagnostic("metronomeNeedle",value);}/></label>
              <label class="toggle-row"><span>"Beat dot pulses"<small>"Independently enables the dot highlight pulses - use alone to test the pulses in the beat-judder bisection."</small></span><input type="checkbox" aria-label="Metronome beat pulses" prop:checked=move || diagnostic_metronome_pulses.get() on:change=move |event|{let value=event_target_checked(&event);diagnostic_metronome_pulses.set(value);set_diagnostic("metronomePulses",value);}/></label>
              <label class="toggle-row"><span>"Auto-scroll camera"<small>"The score page follow-scroll. The playhead keeps moving without it."</small></span><input type="checkbox" aria-label="Auto-scroll camera" prop:checked=move || diagnostic_auto_scroll.get() on:change=move |event|{let value=event_target_checked(&event);diagnostic_auto_scroll.set(value);set_diagnostic("autoScroll",value);}/></label>
              <label class="toggle-row"><span>"Scoring & UI updates"<small>"Disables the ~30 Hz scoring/state work that shares the animation frame. Transport and playhead keep running; the run shows count-in state and its result is NOT saved. This reproduces the once-observed abnormal smooth state on purpose."</small></span><input type="checkbox" aria-label="Scoring and UI updates" prop:checked=move || diagnostic_state_tick.get() on:change=move |event|{let value=event_target_checked(&event);diagnostic_state_tick.set(value);set_diagnostic("stateTick",value);if !value && matches!(phase.get_untracked().as_str(), "count_in"|"starting"|"running"|"paused"){diagnostic_run_invalid.set(true);}}/></label>
              <label class="toggle-row"><span>"Performance counters"<small>"Records callback and feedback timings in memory; read with rudiment.performanceDiagnostics(). Adds a small recording cost while on."</small></span><input type="checkbox" aria-label="Performance counters" prop:checked=move || diagnostic_perf_counters.get() on:change=move |event|{let value=event_target_checked(&event);diagnostic_perf_counters.set(value);set_diagnostic("perfCounters",value);}/></label>
              <div class="button-row"><button on:click=move |_|{set_diagnostic("stateTick",false);diagnostic_state_tick.set(false);set_diagnostic("visualFeedback",false);diagnostic_visual_feedback.set(false);set_diagnostic("metronomeVisual",false);diagnostic_metronome_visual.set(false);set_diagnostic("metronomeNeedle",false);diagnostic_metronome_needle.set(false);set_diagnostic("metronomePulses",false);diagnostic_metronome_pulses.set(false);set_diagnostic("autoScroll",true);diagnostic_auto_scroll.set(true);set_diagnostic("referenceOverlay",true);diagnostic_reference_overlay.set(true);if matches!(phase.get_untracked().as_str(), "count_in"|"starting"|"running"|"paused"){diagnostic_run_invalid.set(true);}}>"Minimal playhead-only"</button><button on:click=move |_|{set_diagnostic("referenceOverlay",diagnostics_state.reference_overlay);diagnostic_reference_overlay.set(diagnostics_state.reference_overlay);set_diagnostic("visualFeedback",diagnostics_state.visual_feedback);diagnostic_visual_feedback.set(diagnostics_state.visual_feedback);set_diagnostic("metronomeVisual",diagnostics_state.metronome_visual);diagnostic_metronome_visual.set(diagnostics_state.metronome_visual);set_diagnostic("metronomeNeedle",diagnostics_state.metronome_needle);diagnostic_metronome_needle.set(diagnostics_state.metronome_needle);set_diagnostic("metronomePulses",diagnostics_state.metronome_pulses);diagnostic_metronome_pulses.set(diagnostics_state.metronome_pulses);set_diagnostic("stateTick",diagnostics_state.state_tick);diagnostic_state_tick.set(diagnostics_state.state_tick);set_diagnostic("autoScroll",diagnostics_state.auto_scroll);diagnostic_auto_scroll.set(diagnostics_state.auto_scroll);set_diagnostic("perfCounters",diagnostics_state.perf_counters);diagnostic_perf_counters.set(diagnostics_state.perf_counters);}>"Reset diagnostics"</button></div>
              <p class="help">{move || format!("Open the independent reference animation at your current tempo ({:.0} BPM) beside Rudiment for a like-for-like smoothness comparison.", bpm.get())}</p>
              <a class="wide-button" href={move || format!("./public/animation-demo.html?bpm={:.0}&method=transform&fit=1&thickness=3", bpm.get())} target="_blank" rel="noopener">"Open reference animation demo"</a>
            </section>
          </div>

                 {move || pending_clear_history.get().then(|| view!{
                   <div class="delete-confirm-backdrop" on:click=move |_|{pending_clear_history.set(false);}>
                     <section id="clear-history-dialog" class="delete-confirm-dialog" role="alertdialog" aria-modal="true" aria-labelledby="clear-history-title" aria-describedby="clear-history-description" on:click=|event|event.stop_propagation()>
                       <span class="eyebrow">"CLEAR HISTORY"</span>
                       <h2 id="clear-history-title">"Clear all performance history?"</h2>
                       <p id="clear-history-description">"All completed performance records and practice time on this browser will be permanently removed."</p>
                       <div class="delete-confirm-actions">
                         <button id="clear-history-cancel" on:click=move |_|{pending_clear_history.set(false);}>"Cancel"</button>
                         <button class="delete-confirm-danger" on:click=move |_|{
                           pending_clear_history.set(false);
                           let promise=clear_history();
                           spawn_local(async move{match JsFuture::from(promise).await{Ok(_)=>{history.set(vec![]);incomplete_history.set(0);practice_time_ms.set(Some(0.0));},Err(error)=>import_error.set(Some(format!("History could not be cleared: {error:?}")))}});
                         }>"Clear history"</button>
                       </div>
                     </section>
                   </div>
                 })}
                 {move || pending_reset_settings.get().then(|| view!{
                   <div class="delete-confirm-backdrop" on:click=move |_|{pending_reset_settings.set(false);}>
                     <section id="reset-settings-dialog" class="delete-confirm-dialog" role="alertdialog" aria-modal="true" aria-labelledby="reset-settings-title" aria-describedby="reset-settings-description" on:click=|event|event.stop_propagation()>
                       <span class="eyebrow">"RESET SETTINGS"</span>
                       <h2 id="reset-settings-title">"Reset settings & mappings?"</h2>
                       <p id="reset-settings-description">"All practice settings, mappings and local setup will be restored to defaults."</p>
                       <div class="delete-confirm-actions">
                         <button id="reset-settings-cancel" on:click=move |_|{pending_reset_settings.set(false);}>"Cancel"</button>
                         <button class="delete-confirm-danger" on:click=move |_|{
                           pending_reset_settings.set(false);
                           let promise=reset_local_setup();
                           app_mode.set(AppMode::Practice);
                           bpm.set(60.0);count_in.set(1);subdivision.set(1);timbre.set("woodblock".into());metronome.set(true);auto_show_statistics.set(true);click_volume.set(0.72);snare_volume.set(0.72);snare_muted.set(false);snare_reference_mode.set("hits".into());loops.set(1);repeat_bar_until_good.set(false);repeat_tolerance.set(RepeatTolerance::Standard);selected_bars.set(vec![]);latency_ms.set(0.0);on_time_ms.set(DEFAULT_GOOD_MS);excellent_ms.set(DEFAULT_PERFECT_MS);snare_note.set(38);snare_channel.set(10);midi_bindings.set(default_midi_bindings());midi_learn.set(None);selected_midi.set(None);midi_status.set("Not connected".into());adaptive_enabled.set(false);adaptive_settings.set(AdaptiveSettings::default());selected_import.set(None);library_page.set(0);library_difficulty.set(None);
                           spawn_local(async move{let _=JsFuture::from(promise).await;});
                         }>"Reset"</button>
                       </div>
                     </section>
                   </div>
                 })}</aside><button class=move || if settings_open.get(){"drawer-backdrop"}else{"drawer-backdrop hidden"} aria-label="Close settings" tabindex="-1" on:click=move |_|{settings_open.set(false);restore_settings_focus();}></button>
                  <div class=move || if midi_modal_open.get(){"midi-modal-backdrop"}else{"midi-modal-backdrop hidden"}>
                    <section
                      id="midi-connection-modal"
                      class="midi-modal"
                      role="dialog"
                      aria-modal="true"
                      aria-labelledby="midi-modal-title"
                      aria-hidden=move || (!midi_modal_open.get()).to_string()
                      inert=move || !midi_modal_open.get()
                      on:click=|event|event.stop_propagation()
                      on:keydown=move |event:web_sys::KeyboardEvent|{
                        match event.key().as_str(){
                          "Escape"=>{event.prevent_default();midi_modal_open.set(false);},
                          "Tab"=>trap_dialog_tab(&event,"#midi-connection-modal"),
                          _=>{},
                        }
                      }
                    >
                      <div class="midi-modal-head">
                        <div><span class="eyebrow">"MIDI DRUMS"</span><h2 id="midi-modal-title">"MIDI connection"</h2></div>
                        <button id="midi-modal-close" aria-label="Close MIDI connection setup" on:click=move |_|{midi_modal_open.set(false);}><X size=22/></button>
                      </div>
                      <p class="midi-modal-instructions">"Connect your drum module with a USB MIDI cable, or pair it over Bluetooth MIDI in your system settings, then choose it below. If your browser asks to use MIDI, allow it so Rudiment can see your kit."</p>
                      <MidiConfigSection midi_status=midi_status midi_ports=midi_ports selected_midi=selected_midi midi_learn=midi_learn midi_bindings=midi_bindings diagnostics=diagnostics midi_modal_open=midi_modal_open connect=connect_action.clone()/>
                      <div class="midi-modal-actions">
                        <button id="midi-modal-continue" on:click=move |_|{midi_modal_open.set(false);}>"Continue without MIDI"</button>
                      </div>
                    </section>
                  </div>
      </main>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rudiment_core::Hand;

    fn port(id: &str, state: &str, connection: &str) -> MidiPort {
        MidiPort {
            id: id.into(),
            name: id.into(),
            manufacturer: "Test".into(),
            state: state.into(),
            connection: connection.into(),
        }
    }

    #[test]
    fn midi_connection_requires_a_connected_selected_port() {
        let ports = vec![port("kit", "connected", "open")];
        assert!(midi_connection_ready(&ports, Some("kit")));
        // Nothing selected, a selection that disappeared, a pending port and
        // a closed connection all mean MIDI is not ready yet.
        assert!(!midi_connection_ready(&ports, None));
        assert!(!midi_connection_ready(&ports, Some("other")));
        assert!(!midi_connection_ready(
            &[port("kit", "disconnected", "pending")],
            Some("kit")
        ));
        assert!(!midi_connection_ready(
            &[port("kit", "connected", "closed")],
            Some("kit")
        ));
    }

    fn observed_at(timestamp_ms: f64) -> ObservedHit {
        ObservedHit {
            timestamp_ms,
            instrument: Instrument::Snare,
            articulation: Articulation::Center,
            velocity: 100,
            source: InputSource::Demo,
            raw_note: None,
            raw_channel: None,
        }
    }

    fn two_note_bar() -> Vec<ExpectedHit> {
        vec![
            ExpectedHit {
                id: "bar-note-1".into(),
                tick: 0,
                beat: 0.0,
                notation_beat: 0.0,
                raw_duration_ticks: Some(120),
                notation_duration_beats: 0.25,
                instrument: Instrument::Snare,
                articulation: Articulation::Center,
                velocity: 100,
                accent: false,
                grace: false,
                hand: Hand::R,
                channel: 9,
                note: 38,
            },
            ExpectedHit {
                id: "bar-note-2".into(),
                tick: 480,
                beat: 1.0,
                notation_beat: 1.0,
                raw_duration_ticks: Some(120),
                notation_duration_beats: 0.25,
                instrument: Instrument::Snare,
                articulation: Articulation::Center,
                velocity: 100,
                accent: false,
                grace: false,
                hand: Hand::L,
                channel: 9,
                note: 38,
            },
        ]
    }

    fn session_record(id: &str, hash: &str, version: &str, score: f64) -> SessionRecord {
        let expected = two_note_bar();
        let mut summary = score_session_with_tempo_map(
            &expected,
            &[observed_at(0.0), observed_at(1_000.0)],
            60.0,
            0.0,
            0.0,
            ScoringConfig::default(),
            &[],
        );
        summary.score = Some(score);
        summary.scoring_version = version.into();
        SessionRecord {
            id: id.into(),
            ended_at: "2026-11-11T18:00:00Z".into(),
            exercise_id: "exercise".into(),
            exercise_title: "Exercise".into(),
            exercise_hash: hash.into(),
            bpm: 60.0,
            input_mode: InputSource::Demo,
            scoring_version: version.into(),
            tolerance_ms: DEFAULT_GOOD_MS,
            latency_ms: 0.0,
            summary,
            observed: vec![],
            schema_version: 1,
            mode: AppMode::Performance,
        }
    }

    fn live_feedback(expected: &[ExpectedHit], timestamps: &[f64]) -> Vec<LiveJudgement> {
        let mut matched = HashSet::new();
        let sequence = BarSequence::new(&paradiddle(), &[0]);
        timestamps
            .iter()
            .map(|timestamp| {
                live_judgement_for_hit(
                    expected,
                    &mut matched,
                    &observed_at(*timestamp),
                    &LiveTimingContext {
                        session_start_ms: 0.0,
                        bpm: 60.0,
                        tempo_map: &[],
                        sequence: &sequence,
                        latency_ms: 0.0,
                        config: ScoringConfig::default(),
                    },
                )
            })
            .collect()
    }

    #[test]
    fn greedy_live_and_final_optimal_assignments_can_disagree() {
        let mut expected = two_note_bar();
        expected[1].beat = 0.08;
        let live = live_feedback(&expected, &[60.0, 150.0]);
        assert_eq!(live[0].expected_id.as_deref(), Some("bar-note-2"));
        assert_eq!(live[1].judgement, Judgement::Extra);
        let final_result = score_session_with_tempo_map(
            &expected,
            &[observed_at(60.0), observed_at(150.0)],
            60.0,
            0.0,
            0.0,
            ScoringConfig::default(),
            &[],
        );
        assert_eq!(final_result.matched, 2);
        assert_eq!(final_result.results[0].observed_ms, Some(60.0));
        assert_eq!(final_result.results[1].observed_ms, Some(150.0));
    }

    #[test]
    fn live_and_final_can_disagree_on_wrong_instrument_after_a_match() {
        let expected = vec![two_note_bar()[0].clone()];
        let mut kick = observed_at(0.0);
        kick.instrument = Instrument::Kick;
        let sequence = BarSequence::new(&paradiddle(), &[0]);
        let timing = LiveTimingContext {
            session_start_ms: 0.0,
            bpm: 60.0,
            tempo_map: &[],
            sequence: &sequence,
            latency_ms: 0.0,
            config: ScoringConfig::default(),
        };
        let mut matched = HashSet::new();
        live_judgement_for_hit(&expected, &mut matched, &observed_at(0.0), &timing);
        let live = live_judgement_for_hit(&expected, &mut matched, &kick, &timing);
        assert_eq!(live.judgement, Judgement::Extra);
        let final_result = score_session_with_tempo_map(
            &expected,
            &[observed_at(0.0), kick],
            60.0,
            0.0,
            0.0,
            ScoringConfig::default(),
            &[],
        );
        assert_eq!(final_result.wrong_instrument, 1);
        assert_eq!(
            final_result.results[1].judgement,
            Judgement::WrongInstrument
        );
    }

    // Native measurement of the actual live matcher, not the browser callback.
    // Deliberately opt-in; use a release build for meaningful timings.
    #[test]
    #[ignore]
    fn scoring_study_live_matcher_benchmark() {
        use std::time::Instant;
        for (count, spacing, gap) in [
            (500, 1000.0, false),
            (500, 250.0, false),
            (500, 125.0, false),
            (500, 62.5, false),
            (1000, 31.25, false),
            (2000, 62.5, false),
            (10000, 62.5, false),
            (2000, 62.5, true),
            (10000, 62.5, true),
            (500, 0.0, false),
        ] {
            let template = two_note_bar()[0].clone();
            let expected: Vec<_> = (0..count)
                .map(|i| ExpectedHit {
                    id: format!("n{i}-loop{}", i / 16 + 1),
                    beat: i as f64 * spacing / 1000.0,
                    notation_beat: i as f64 * spacing / 1000.0,
                    ..template.clone()
                })
                .collect();
            let sequence = BarSequence::new(&paradiddle(), &[0]);
            let timing = LiveTimingContext {
                session_start_ms: 0.0,
                bpm: 60.0,
                tempo_map: &[],
                sequence: &sequence,
                latency_ms: 0.0,
                config: ScoringConfig::default(),
            };
            let mut matched = HashSet::new();
            let mut samples = Vec::with_capacity(count);
            for i in 0..count {
                if gap && i == count / 4 {
                    continue;
                }
                let hit = observed_at(i as f64 * spacing + 5.0);
                let start = Instant::now();
                std::hint::black_box(live_judgement_for_hit(
                    &expected,
                    &mut matched,
                    &hit,
                    &timing,
                ));
                samples.push(start.elapsed().as_secs_f64() * 1000.0);
            }
            samples.sort_by(f64::total_cmp);
            let p =
                |fraction: f64| samples[((samples.len() - 1) as f64 * fraction).ceil() as usize];
            println!(
                "live count={count} spacing_ms={spacing:.2} gap={gap} p50_ms={:.4} p95_ms={:.4} p99_ms={:.4} max_ms={:.4}",
                p(0.5),
                p(0.95),
                p(0.99),
                p(1.0)
            );
        }
    }

    #[test]
    fn reference_drums_preserve_simultaneous_instruments_in_the_active_segment() {
        let mut expected = two_note_bar();
        expected[0].velocity = 42;
        let mut duplicate = expected[1].clone();
        duplicate.id = "bar-note-2-accent".into();
        duplicate.velocity = 118;
        expected.push(duplicate);
        let mut kick = expected[0].clone();
        kick.id = "kick".into();
        kick.beat = 0.5;
        kick.instrument = Instrument::Kick;
        kick.articulation = Articulation::Center;
        expected.push(kick);
        let mut next_bar = expected[0].clone();
        next_bar.id = "next-bar".into();
        next_bar.beat = 4.0;
        expected.push(next_bar);

        assert_eq!(
            reference_drum_notes(&expected, 0.0, 4.0),
            vec![
                ReferenceDrumNote {
                    beat: 0.0,
                    velocity: 42,
                    instrument: Instrument::Snare,
                    articulation: Articulation::Center,
                },
                ReferenceDrumNote {
                    beat: 0.5,
                    velocity: 42,
                    instrument: Instrument::Kick,
                    articulation: Articulation::Center,
                },
                ReferenceDrumNote {
                    beat: 1.0,
                    velocity: 118,
                    instrument: Instrument::Snare,
                    articulation: Articulation::Center,
                },
            ]
        );
    }

    #[test]
    fn live_tempo_change_preserves_elapsed_history_and_retimes_future_beats() {
        let exercise = paradiddle();
        let original = scaled_tempo_map(&exercise, 1, 60.0);
        let sequence = BarSequence::new(&exercise, &[]);
        let changed = retime_tempo_map(&original, &exercise, &sequence, 1, 4.0, 120.0);
        let base = changed[0].bpm;
        assert!((elapsed_ms_at_beat(4.0, base, &changed) - 4_000.0).abs() < 0.01);
        assert!((elapsed_ms_at_beat(8.0, base, &changed) - 6_000.0).abs() < 0.01);
        assert_eq!(
            changed
                .iter()
                .find(|change| change.beat == 4.0)
                .unwrap()
                .bpm,
            120.0
        );
    }

    #[test]
    fn bar_passes_only_when_every_note_is_perfect_or_good() {
        let expected = two_note_bar();
        let perfect_and_good = live_feedback(&expected, &[0.0, 1_025.0]);
        assert!(bar_attempt_is_good(
            2,
            2,
            &perfect_and_good,
            RepeatTolerance::Standard
        ));

        let early = live_feedback(&expected, &[0.0, 939.0]);
        assert!(!bar_attempt_is_good(
            2,
            2,
            &early,
            RepeatTolerance::Standard
        ));
    }

    #[test]
    fn final_note_late_window_stays_with_its_bar_across_the_boundary() {
        let mut expected = two_note_bar();
        expected[1].beat = 3.9;
        let attempt = hits_for_bar_attempt(&expected, 0.0, 4.0);
        let target = elapsed_ms_at_beat(3.9, 120.0, &[]);
        let mut matched = HashSet::new();
        let sequence = BarSequence::new(&paradiddle(), &[0]);
        let judgement = live_judgement_for_hit(
            &attempt,
            &mut matched,
            &ObservedHit {
                timestamp_ms: target + 75.0,
                ..observed_at(target + 75.0)
            },
            &LiveTimingContext {
                session_start_ms: 0.0,
                bpm: 120.0,
                tempo_map: &[],
                sequence: &sequence,
                latency_ms: 0.0,
                config: ScoringConfig::default(),
            },
        );

        assert!(target + 75.0 > elapsed_ms_at_beat(4.0, 120.0, &[]));
        assert_eq!(judgement.expected_id.as_deref(), Some("bar-note-2"));
        assert_eq!(judgement.judgement, Judgement::Late);
    }

    #[test]
    fn repeat_segments_keep_the_paused_offset_without_moving_bar_boundaries() {
        for (offset, start, end) in [
            (0.25, 0.0, 4.0),
            (3.999, 0.0, 4.0),
            (4.25, 4.0, 8.0),
            (15.9, 12.0, 16.0),
        ] {
            assert_eq!(repeat_bar_segment(offset, 4.0, 16.0), (start, end));
        }
    }

    #[test]
    fn resuming_rebases_keyboard_and_midi_hits_without_counting_paused_time() {
        for bpm in [60.0, 72.0, 120.0, 240.0] {
            for beat in [0.25, 1.5, 3.999] {
                let mut hits = [
                    ObservedHit {
                        source: InputSource::Demo,
                        ..observed_at(1_000.0 + elapsed_ms_at_beat(beat, bpm, &[]))
                    },
                    ObservedHit {
                        source: InputSource::Midi,
                        ..observed_at(1_000.0 + elapsed_ms_at_beat(beat, bpm, &[]))
                    },
                ];
                let pause_at = 1_000.0 + elapsed_ms_at_beat(beat + 0.1, bpm, &[]);
                let resume_at = pause_at + 5_000.0;
                let resumed_origin = resume_at - elapsed_ms_at_beat(beat + 0.1, bpm, &[]);

                rebase_hits_for_resume(&mut hits, 1_000.0, resumed_origin);

                let expected_hit_time = resumed_origin + elapsed_ms_at_beat(beat, bpm, &[]);
                assert!(
                    hits.iter()
                        .all(|hit| (hit.timestamp_ms - expected_hit_time).abs() < 0.001)
                );
            }
        }
    }

    #[test]
    fn missing_or_extra_hits_fail_a_bar_attempt() {
        let expected = two_note_bar();
        let missing = live_feedback(&expected, &[0.0]);
        assert!(!bar_attempt_is_good(
            2,
            1,
            &missing,
            RepeatTolerance::Standard
        ));

        let extra = live_feedback(&expected, &[0.0, 1_000.0, 1_500.0]);
        assert!(!bar_attempt_is_good(
            2,
            2,
            &extra,
            RepeatTolerance::Standard
        ));
    }

    #[test]
    fn an_empty_bar_passes_when_the_player_does_not_add_a_hit() {
        assert!(bar_attempt_is_good(0, 0, &[], RepeatTolerance::Standard));

        let extra = live_feedback(&[], &[500.0]);
        assert!(!bar_attempt_is_good(
            0,
            0,
            &extra,
            RepeatTolerance::Standard
        ));
    }

    #[test]
    fn live_feedback_reports_only_physical_hits_and_never_synthesises_misses() {
        let expected = two_note_bar();
        let feedback = live_feedback(&expected, &[0.0]);
        assert_eq!(feedback.len(), 1);
        assert_eq!(feedback[0].expected_id.as_deref(), Some("bar-note-1"));
        assert_eq!(feedback[0].judgement, Judgement::Perfect);
        assert!(
            feedback
                .iter()
                .all(|hit| hit.judgement != Judgement::Missed)
        );
    }

    #[test]
    fn repeated_bar_feedback_stays_on_the_selected_staff() {
        let expected = vec![ExpectedHit {
            id: "bar-eight-loop2".into(),
            beat: 32.0,
            ..two_note_bar()[0].clone()
        }];
        let mut matched = HashSet::new();
        let sequence = BarSequence {
            bars: vec![7],
            bar_beats: 4.0,
            origin: 28.0,
            total_bars: 8,
        };
        let result = live_judgement_for_hit(
            &expected,
            &mut matched,
            &observed_at(8_000.0),
            &LiveTimingContext {
                session_start_ms: 0.0,
                bpm: 240.0,
                tempo_map: &[],
                sequence: &sequence,
                latency_ms: 0.0,
                config: ScoringConfig::default(),
            },
        );
        assert_eq!(result.expected_beat, Some(28.0));
        assert_eq!(result.actual_beat, Some(28.0));
    }

    #[test]
    fn repeat_tolerance_changes_acceptance_without_changing_classification() {
        let expected = two_note_bar();
        let early = live_feedback(&expected, &[0.0, 1_061.0]);
        assert_eq!(early[1].judgement, Judgement::Late);
        assert!(bar_attempt_is_good(2, 2, &early, RepeatTolerance::Relaxed));
        assert!(!bar_attempt_is_good(
            2,
            2,
            &early,
            RepeatTolerance::Standard
        ));
        let good = live_feedback(&expected, &[0.0, 1_025.0]);
        assert!(bar_attempt_is_good(2, 2, &good, RepeatTolerance::Standard));
        assert!(!bar_attempt_is_good(2, 2, &good, RepeatTolerance::Strict));
    }

    #[test]
    fn selected_bars_keep_score_order_and_contiguous_scoring_time() {
        let mut exercise = paradiddle();
        let mut fifth = exercise.hits[0].clone();
        fifth.id = "fifth-bar".into();
        fifth.beat = 16.0;
        fifth.notation_beat = 16.0;
        exercise.hits.push(fifth);
        let sequence = BarSequence::new(&exercise, &[4, 1, 2]);
        assert_eq!(sequence.bars, vec![1, 2, 4]);
        assert_eq!(sequence.origin, 4.0);
        for (timeline, score) in [
            (4.0, 4.0),
            (8.0, 8.0),
            (12.0, 16.0),
            (16.0, 4.0),
            (20.0, 8.0),
            (24.0, 16.0),
        ] {
            assert_eq!(sequence.display_beat(timeline), score);
        }
        let hits = expanded_sequence_hits(&exercise, &sequence, 2);
        assert!(hits.iter().all(|hit| (4.0..28.0).contains(&hit.beat)));
        assert_eq!(
            hits.iter().map(|hit| &hit.id).collect::<HashSet<_>>().len(),
            hits.len()
        );
        assert!(
            hits.iter()
                .any(|hit| hit.id == "fifth-bar-loop1" && hit.beat == 12.0)
        );
        assert!(
            hits.iter()
                .any(|hit| hit.id == "fifth-bar-loop2" && hit.beat == 24.0)
        );
        let observed = hits
            .iter()
            .map(|hit| observed_at(1_000.0 + hit.beat * 250.0))
            .collect::<Vec<_>>();
        let summary = score_session_with_tempo_map(
            &hits,
            &observed,
            240.0,
            1_000.0,
            0.0,
            ScoringConfig::default(),
            &[],
        );
        assert_eq!(summary.matched, hits.len());
        assert_eq!(summary.missed, 0);
        assert_eq!(summary.extras, 0);
        assert_eq!(BarSequence::new(&exercise, &[]).bars, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn presentation_uses_scoring_bar_membership_for_early_grace_notes() {
        let mut exercise = paradiddle();
        let mut grace = exercise.hits[0].clone();
        grace.id = "early-grace".into();
        grace.beat = 7.99;
        grace.notation_beat = 8.0;
        grace.grace = true;
        exercise.hits.push(grace);
        let source = presentation_exercise(&exercise);
        let presented = source
            .hits
            .iter()
            .find(|item| item.hit.id == "early-grace")
            .unwrap();
        assert_eq!(presented.source_bar, 2);
        assert_eq!(presented.hit.beat, 7.99);
        let serialized = serde_json::to_value(&source).unwrap();
        let serialized_grace = serialized["hits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["id"] == "early-grace")
            .unwrap();
        assert_eq!(serialized_grace["source_bar"], 2);
        assert_eq!(serialized_grace["beat"], 7.99);
        let sequence = BarSequence::new(&exercise, &[2]);
        let expanded = expanded_sequence_hits(&exercise, &sequence, 1);
        assert!(
            expanded
                .iter()
                .any(|hit| hit.id == "early-grace-loop1" && hit.beat == 7.99)
        );
    }

    #[test]
    fn notes_immediately_after_a_loop_boundary_keep_distinct_ids_and_match() {
        let exercise = paradiddle();
        let sequence = BarSequence::new(&exercise, &[]);
        let hits = expanded_sequence_hits(&exercise, &sequence, 2);
        let boundary = hits
            .iter()
            .filter(|hit| (hit.beat - 16.0).abs() < 0.001 || (hit.beat - 16.5).abs() < 0.001)
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            boundary
                .iter()
                .map(|hit| hit.id.as_str())
                .collect::<Vec<_>>(),
            vec!["p-0-0-loop2", "p-0-1-loop2"]
        );
        let observed = boundary
            .iter()
            .map(|hit| observed_at(hit.beat * 1_000.0))
            .collect::<Vec<_>>();
        let summary = score_session_with_tempo_map(
            &boundary,
            &observed,
            60.0,
            0.0,
            0.0,
            ScoringConfig::default(),
            &[],
        );
        assert_eq!(summary.matched, 2);
        assert_eq!(summary.missed, 0);
        assert_eq!(summary.extras, 0);
    }

    #[test]
    fn selected_bar_jumps_preserve_source_tempos_and_grace_offsets() {
        let mut exercise = paradiddle();
        let mut principal = exercise.hits[0].clone();
        principal.id = "principal".into();
        principal.beat = 16.0;
        principal.notation_beat = 16.0;
        let mut grace = principal.clone();
        grace.id = "grace".into();
        grace.beat = 15.875;
        grace.grace = true;
        exercise.hits.extend([grace, principal]);
        exercise.tempo_map = [(0.0, 60.0), (4.0, 120.0), (8.0, 90.0), (16.0, 180.0)]
            .into_iter()
            .map(|(beat, bpm)| TempoChange {
                tick: (beat * exercise.ticks_per_beat as f64) as u64,
                beat,
                bpm,
            })
            .collect();
        let sequence = BarSequence::new(&exercise, &[1, 2, 4]);
        let tempo = expanded_sequence_tempo_map(&exercise, &sequence, 2);
        assert_eq!(
            tempo
                .iter()
                .map(|change| (change.beat, change.bpm))
                .collect::<Vec<_>>(),
            vec![
                (4.0, 120.0),
                (8.0, 90.0),
                (12.0, 180.0),
                (16.0, 120.0),
                (20.0, 90.0),
                (24.0, 180.0)
            ]
        );
        let hits = expanded_sequence_hits(&exercise, &sequence, 2);
        assert!(
            hits.iter()
                .any(|hit| hit.id == "grace-loop1" && hit.beat == 11.875)
        );
        assert!(
            hits.iter()
                .any(|hit| hit.id == "principal-loop1" && hit.beat == 12.0)
        );
        assert!(
            hits.iter()
                .any(|hit| hit.id == "grace-loop2" && hit.beat == 23.875)
        );
    }

    #[test]
    fn legacy_bar_ranges_migrate_to_individual_selections() {
        let exercise = paradiddle();
        assert_eq!(legacy_selected_bars(&exercise, 1, 2), vec![1, 2]);
        assert_eq!(
            legacy_selected_bars(&exercise, 1, usize::MAX),
            vec![1, 2, 3]
        );
        assert!(legacy_selected_bars(&exercise, 0, usize::MAX).is_empty());
    }

    #[test]
    fn performance_stars_are_deterministic_and_penalise_extras() {
        let expected = two_note_bar();
        let perfect = score_session_with_tempo_map(
            &expected,
            &[observed_at(0.0), observed_at(1_000.0)],
            60.0,
            0.0,
            0.0,
            ScoringConfig::default(),
            &[],
        );
        assert_eq!(
            performance_rating(&perfect).award,
            PerformanceAward::Mastered
        );
        let with_extra = score_session_with_tempo_map(
            &expected,
            &[observed_at(0.0), observed_at(1_000.0), observed_at(1_500.0)],
            60.0,
            0.0,
            0.0,
            ScoringConfig::default(),
            &[],
        );
        assert_ne!(
            performance_rating(&with_extra).award,
            PerformanceAward::Mastered
        );
        assert!(performance_rating(&with_extra).stars < 3);
    }

    #[test]
    fn new_high_score_is_strict_and_scoped_to_the_midi_file() {
        let current = session_record("current", "same-file", SCORING_VERSION, 86.0);
        assert!(!is_new_high_score(&current, std::slice::from_ref(&current)));

        let lower = session_record("lower", "same-file", SCORING_VERSION, 85.0);
        assert!(is_new_high_score(
            &current,
            &[lower.clone(), current.clone()]
        ));
        assert_eq!(lower.summary.score, Some(85.0));

        let tied = session_record("tied", "same-file", SCORING_VERSION, 86.0);
        assert!(!is_new_high_score(&current, &[tied, current.clone()]));

        let higher = session_record("higher", "same-file", SCORING_VERSION, 90.0);
        assert!(!is_new_high_score(&current, &[higher, current.clone()]));

        let other_file = session_record("other", "other-file", SCORING_VERSION, 100.0);
        let old_version = session_record("old", "same-file", "v2", 100.0);
        assert!(!is_new_high_score(
            &current,
            &[other_file, old_version, current.clone()]
        ));
    }

    #[test]
    fn white_mastery_requires_a_clean_perfect_or_good_run() {
        let expected = two_note_bar();
        let clean = score_session_with_tempo_map(
            &expected,
            &[observed_at(0.0), observed_at(1_025.0)],
            60.0,
            0.0,
            0.0,
            ScoringConfig::default(),
            &[],
        );
        assert!(is_white_mastery(&clean));

        let early = score_session_with_tempo_map(
            &expected,
            &[observed_at(0.0), observed_at(1_061.0)],
            60.0,
            0.0,
            0.0,
            ScoringConfig::default(),
            &[],
        );
        assert!(!is_white_mastery(&early));

        let extra = score_session_with_tempo_map(
            &expected,
            &[observed_at(0.0), observed_at(1_000.0), observed_at(1_500.0)],
            60.0,
            0.0,
            0.0,
            ScoringConfig::default(),
            &[],
        );
        assert!(!is_white_mastery(&extra));
    }

    #[test]
    fn progress_includes_all_prior_scores_for_the_same_exercise() {
        let mut history = (0..12)
            .map(|index| {
                let mut session = session_record(
                    &format!("session-{index:02}"),
                    "same-file",
                    SCORING_VERSION,
                    index as f64,
                );
                session.ended_at = format!("2026-11-11T18:{index:02}:00Z");
                session
            })
            .collect::<Vec<_>>();
        let current = history.last().unwrap().clone();
        let mut other_tempo = session_record("other-tempo", "same-file", SCORING_VERSION, 99.0);
        other_tempo.bpm = 120.0;
        other_tempo.ended_at = "2026-11-11T18:05:30Z".into();
        history.push(other_tempo);
        let progress = progress_sessions(&current, &history);
        assert_eq!(progress.len(), 13);
        assert_eq!(progress[6], ("other-tempo".into(), 99.0));
        assert_eq!(progress.last().unwrap().0, current.id);

        let lone = session_record("lone", "new-file", SCORING_VERSION, 78.0);
        assert_eq!(progress_sessions(&lone, &[]), vec![("lone".into(), 78.0)]);
        assert_eq!(progress_chart_points(&[78.0]).len(), 1);
        assert_eq!(progress_chart_points(&[60.0, 80.0]).len(), 2);
        assert_eq!(
            progress_sessions(&history[2], &history)
                .iter()
                .map(|(_, score)| *score)
                .collect::<Vec<_>>(),
            vec![0.0, 1.0, 2.0]
        );
        let mut unrelated = session_record("unrelated", "different-file", SCORING_VERSION, 100.0);
        unrelated.exercise_title = current.exercise_title.clone();
        assert!(!same_exercise(&current, &unrelated));
        let mut legacy = current.clone();
        legacy.exercise_hash.clear();
        assert!(same_exercise(&current, &legacy));
    }

    #[test]
    fn builtin_library_levels_have_valid_midi_and_simple_beginner_patterns() {
        assert_eq!(builtin_count(Some(Difficulty::Beginner)), 2);
        assert_eq!(builtin_count(Some(Difficulty::Intermediate)), 7);
        assert_eq!(builtin_count(Some(Difficulty::Advanced)), 4);
        for item in BUILTIN_RUDIMENTS {
            let path = format!(
                "{}/public/exercises/rudiments/{}.mid",
                env!("CARGO_MANIFEST_DIR"),
                item.id
            );
            let data = std::fs::read(path).expect("built-in MIDI is present");
            let exercise = parse_midi(&data, item.title, None).expect("built-in MIDI parses");
            assert!(!exercise.hits.is_empty(), "{}", item.title);
            if item.difficulty == Difficulty::Beginner {
                assert!(
                    exercise
                        .hits
                        .iter()
                        .all(|hit| !hit.grace && hit.hand != rudiment_core::Hand::Unknown)
                );
                assert!(exercise.hits.last().unwrap().beat < 8.0);
                let expected_spacing = if item.id == "12_beginner_single_strokes" {
                    1.0
                } else {
                    0.5
                };
                assert!(
                    exercise
                        .hits
                        .windows(2)
                        .all(
                            |pair| ((pair[1].beat - pair[0].beat) - expected_spacing).abs() < 0.001
                        )
                );
            }
        }
    }

    #[test]
    fn practice_time_formats_accumulated_minutes_and_hours() {
        assert_eq!(format_practice_time(0.0), "0 minutes");
        assert_eq!(format_practice_time(60_000.0), "1 minute");
        assert_eq!(format_practice_time(12.0 * 60_000.0), "12 minutes");
        assert_eq!(format_practice_time(85.0 * 60_000.0), "1 hour 25 minutes");
        assert_eq!(
            format_practice_time(850.0 * 60_000.0),
            "14 hours 10 minutes"
        );
    }

    #[test]
    fn failed_repeat_attempts_remain_in_the_score() {
        let expected = two_note_bar();
        let mut summary = score_session_with_tempo_map(
            &expected,
            &[observed_at(0.0), observed_at(1_000.0)],
            60.0,
            0.0,
            0.0,
            ScoringConfig::default(),
            &[],
        );
        let failed = live_feedback(&expected, &[0.0]);
        include_failed_repeat_attempts(&mut summary, &[(2, failed)]);
        assert_eq!(summary.expected, 4);
        assert_eq!(summary.matched, 3);
        assert_eq!(summary.missed, 1);
        assert_eq!(summary.score, Some(75.0));
    }

    #[test]
    fn live_projection_bounds_unmatched_markers_without_losing_expected_feedback() {
        let expected = LiveJudgement {
            expected_id: Some("expected-note".into()),
            iteration: Some(0),
            expected_beat: Some(0.0),
            actual_beat: None,
            instrument: Instrument::Snare,
            judgement: Judgement::Missed,
            error_ms: 0.0,
            excellent: false,
            note_score: 0.0,
            event_id: Some("expected-note:none:0".into()),
            confirmed: true,
        };
        let mut update = vec![expected];
        update.extend((0..300).map(|index| LiveJudgement {
            expected_id: None,
            iteration: None,
            expected_beat: None,
            actual_beat: Some(index as f64),
            instrument: Instrument::Snare,
            judgement: Judgement::Extra,
            error_ms: 0.0,
            excellent: false,
            note_score: 0.0,
            event_id: Some(format!("extra-{index}")),
            confirmed: true,
        }));

        let mut visible = Vec::new();
        merge_live_feedback(&mut visible, update);

        assert_eq!(visible.len(), LIVE_UNMATCHED_FEEDBACK_LIMIT + 1);
        assert!(
            visible
                .iter()
                .any(|judgement| judgement.expected_id.as_deref() == Some("expected-note"))
        );
        let unmatched = visible
            .iter()
            .filter(|judgement| judgement.expected_id.is_none())
            .collect::<Vec<_>>();
        assert_eq!(unmatched.len(), LIVE_UNMATCHED_FEEDBACK_LIMIT);
        assert_eq!(unmatched[0].event_id.as_deref(), Some("extra-44"));
        assert_eq!(
            unmatched.last().unwrap().event_id.as_deref(),
            Some("extra-299")
        );
    }

    #[test]
    fn live_projection_merge_keeps_only_the_latest_value_for_each_key() {
        let judgement = |expected_id: Option<&str>,
                         iteration: Option<usize>,
                         event_id: Option<&str>,
                         judgement: Judgement,
                         confirmed: bool| LiveJudgement {
            expected_id: expected_id.map(str::to_string),
            iteration,
            expected_beat: expected_id.map(|_| 1.0),
            actual_beat: Some(1.0),
            instrument: Instrument::Snare,
            judgement,
            error_ms: 0.0,
            excellent: judgement == Judgement::Perfect,
            note_score: 0.0,
            event_id: event_id.map(str::to_string),
            confirmed,
        };
        let mut visible = vec![
            judgement(
                Some("expected"),
                Some(0),
                Some("old-expected"),
                Judgement::Perfect,
                true,
            ),
            judgement(None, None, Some("extra"), Judgement::Extra, true),
            judgement(None, None, Some("stale"), Judgement::Extra, false),
        ];
        let update = vec![
            judgement(
                Some("expected"),
                Some(0),
                Some("first-expected"),
                Judgement::Good,
                false,
            ),
            judgement(
                Some("expected"),
                Some(0),
                Some("latest-expected"),
                Judgement::Late,
                false,
            ),
            judgement(None, None, Some("extra"), Judgement::WrongInstrument, false),
            judgement(None, None, Some("extra"), Judgement::Extra, false),
        ];

        merge_live_feedback(&mut visible, update);

        assert_eq!(visible.len(), 2);
        assert_eq!(visible[0].event_id.as_deref(), Some("latest-expected"));
        assert_eq!(visible[0].judgement, Judgement::Late);
        assert_eq!(visible[1].event_id.as_deref(), Some("extra"));
        assert_eq!(visible[1].judgement, Judgement::Extra);
    }

    #[test]
    fn production_uses_one_incremental_scorer_constructor() {
        let production_source = include_str!("main.rs")
            .split("#[cfg(test)]\nmod tests")
            .next()
            .expect("production source");
        assert_eq!(
            production_source.matches("IncrementalScorer::new(").count(),
            1
        );
    }
}
