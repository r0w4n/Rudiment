use js_sys::{Array, Date, Function, Promise, Uint8Array};
use leptos::prelude::*;
use lucide_leptos::{
    ArrowLeft, ArrowRight, ChartNoAxesCombined, FolderOpen, Info, Minus, Pause, Play, Plus,
    RotateCcw, Settings as SettingsIcon, Star, X,
};
use rudiment_core::{
    DEFAULT_GOOD_MS, DEFAULT_MATCH_MS, DEFAULT_PERFECT_MS, Exercise, ExpectedHit, HitResult,
    InputSource, Instrument, Judgement, ObservedHit, PerformanceAward, PerformanceRating,
    SCORING_VERSION, ScoringConfig, SessionSummary, TempoChange, beat_at_elapsed_ms,
    elapsed_ms_at_beat, paradiddle, parse_midi, performance_rating, score_session_with_tempo_map,
};
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
type LaunchRun = Rc<dyn Fn(f64, u8, bool)>;
type LaunchSlot = Rc<RefCell<Option<LaunchRun>>>;
type HitInput = Rc<dyn Fn(f64, Instrument, u8, InputSource, Option<u8>, Option<u8>)>;
type MidiFileCallback = Closure<dyn FnMut(Uint8Array, String, String, JsValue)>;

#[derive(Clone, Copy)]
struct BuiltinRudiment {
    id: &'static str,
    title: &'static str,
    description: &'static str,
}

const BUILTIN_RUDIMENTS: [BuiltinRudiment; 11] = [
    BuiltinRudiment {
        id: "01_roll",
        title: "Roll",
        description: "Alternating double strokes; long-roll mechanism.",
    },
    BuiltinRudiment {
        id: "02_mother_or_5_s_roll",
        title: "Mother or 5 S. Roll",
        description: "Four internal strokes followed by an accented release.",
    },
    BuiltinRudiment {
        id: "03_7_stroke_roll",
        title: "7 Stroke Roll",
        description: "Three doubles and an accented finishing tap.",
    },
    BuiltinRudiment {
        id: "04_9_stroke_roll",
        title: "9 Stroke Roll",
        description: "Four doubles and an accented sustained finishing tap.",
    },
    BuiltinRudiment {
        id: "05_hard_flams",
        title: "Hard Flams",
        description: "Alternating flams with sounded grace notes.",
    },
    BuiltinRudiment {
        id: "06_single_paradiddle",
        title: "Single Paradiddle",
        description: "RLRR / LRLL with four-note accents.",
    },
    BuiltinRudiment {
        id: "07_double_paradiddle",
        title: "Double Paradiddle",
        description: "RLRLRR / LRLRLL in eighth-note triplets.",
    },
    BuiltinRudiment {
        id: "08_flam_paradiddle",
        title: "Flam Paradiddle",
        description: "Paradiddles with opposing-hand flam grace strokes.",
    },
    BuiltinRudiment {
        id: "09_single_drag",
        title: "Single Drag",
        description: "Two opposing-hand grace strokes before each tap.",
    },
    BuiltinRudiment {
        id: "10_single_rotamacue",
        title: "Single Rotamacue",
        description: "Drag and triplet cells using the historical spelling.",
    },
    BuiltinRudiment {
        id: "11_advanced_the_drummers_call_study",
        title: "The Drummers Call — Advanced Study",
        description: "An original eight-bar study inspired by the historical call.",
    },
];

const LIBRARY_PAGE_SIZE: usize = 6;
const CONTINUOUS_LOOP_CHUNK: u8 = 240;

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
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = rudiment, js_name = ensureAudio)]
    fn ensure_audio() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = previewClick)]
    fn preview_click(timbre: &str) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = playSnare)]
    fn play_snare(velocity: u8) -> bool;
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
    #[wasm_bindgen(js_namespace = rudiment, js_name = startCalibration)]
    fn start_calibration() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = renderScore)]
    fn render_score(exercise: JsValue);
    #[wasm_bindgen(js_namespace = rudiment, js_name = setSelectedBars)]
    fn set_selected_bars(start: usize, end: usize);
    #[wasm_bindgen(js_namespace = rudiment, js_name = scrollScoreToBeat)]
    fn scroll_score_to_beat(beat: f64);
    #[wasm_bindgen(js_namespace = rudiment, js_name = updateScore)]
    fn update_score(beat: f64, judgements: JsValue);
    #[wasm_bindgen(js_namespace = rudiment, js_name = appendScoreJudgement)]
    fn append_score_judgement(beat: f64, judgement: JsValue);
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
    fn choose_midi(callback: &Function);
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadBundled)]
    fn load_bundled(url: &str, callback: &Function, save_to_library: bool, display_name: &str);
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadBundledFromLibrary)]
    fn load_bundled_from_library(url: &str, display_name: &str);
    #[wasm_bindgen(js_namespace = rudiment, js_name = bindDrop)]
    fn bind_drop(callback: &Function);
    #[wasm_bindgen(js_namespace = rudiment, js_name = trapDialogTab)]
    fn trap_dialog_tab(event: &web_sys::KeyboardEvent, selector: &str);
    #[wasm_bindgen(js_namespace = rudiment, js_name = saveSession)]
    fn save_session(value: JsValue) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadSessions)]
    fn load_sessions() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = clearHistory)]
    fn clear_history() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = exportHistory)]
    fn export_history() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = saveSettings)]
    fn save_settings(value: JsValue) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadSettings)]
    fn load_settings() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = saveProfile)]
    fn save_profile(value: JsValue) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = resetLocalSetup)]
    fn reset_local_setup() -> Promise;
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
    latency_ms: Option<f64>,
    on_time_ms: Option<f64>,
    excellent_ms: Option<f64>,
    snare_note: Option<u8>,
    snare_channel: Option<u8>,
    welcome_dismissed: Option<bool>,
    last_import_hash: Option<String>,
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
struct ReferenceSnareNote {
    beat: f64,
    velocity: u8,
}

fn reference_snare_notes(
    expected: &[ExpectedHit],
    start_beat: f64,
    end_beat: f64,
) -> Vec<ReferenceSnareNote> {
    let mut notes = expected
        .iter()
        .filter(|hit| {
            hit.instrument == Instrument::Snare
                && hit.beat >= start_beat - 0.000_001
                && hit.beat < end_beat - 0.000_001
        })
        .map(|hit| ReferenceSnareNote {
            beat: hit.beat,
            velocity: hit.velocity.max(1),
        })
        .collect::<Vec<_>>();
    notes.sort_by(|a, b| a.beat.total_cmp(&b.beat));
    notes.dedup_by(|later, earlier| {
        if (later.beat - earlier.beat).abs() < 0.000_001 {
            earlier.velocity = earlier.velocity.max(later.velocity);
            true
        } else {
            false
        }
    });
    notes
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SessionRecord {
    id: String,
    ended_at: String,
    exercise_id: String,
    exercise_title: String,
    exercise_hash: String,
    bpm: f64,
    input_mode: InputSource,
    scoring_version: String,
    tolerance_ms: f64,
    latency_ms: f64,
    summary: SessionSummary,
    observed: Vec<ObservedHit>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LiveJudgement {
    expected_id: Option<String>,
    expected_beat: Option<f64>,
    actual_beat: Option<f64>,
    instrument: Instrument,
    judgement: Judgement,
    error_ms: f64,
    excellent: bool,
    note_score: f64,
    event_id: Option<String>,
}

fn beats_per_bar(exercise: &Exercise) -> f64 {
    exercise.meter_numerator as f64 * 4.0 / exercise.meter_denominator.max(1) as f64
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

fn progress_scores(current: &SessionRecord, history: &[SessionRecord]) -> Vec<f64> {
    let mut matching = history
        .iter()
        .filter(|session| {
            session.exercise_hash == current.exercise_hash
                && (session.bpm - current.bpm).abs() < 0.01
                && session.scoring_version == current.scoring_version
                && session.summary.score.is_some()
        })
        .collect::<Vec<_>>();
    matching.sort_by(|left, right| {
        left.ended_at
            .cmp(&right.ended_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    let mut scores = matching
        .into_iter()
        .filter_map(|session| session.summary.score)
        .collect::<Vec<_>>();
    if !history.iter().any(|session| session.id == current.id)
        && let Some(score) = current.summary.score
    {
        scores.push(score);
    }
    if scores.len() > 10 {
        scores.drain(..scores.len() - 10);
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

fn selected_bar_range(
    exercise: &Exercise,
    requested_start: usize,
    requested_end: usize,
) -> (usize, usize, f64, f64) {
    let bar_beats = beats_per_bar(exercise);
    let exercise_length = exercise_beats(exercise);
    let bar_count = (exercise_length / bar_beats).ceil().max(1.0) as usize;
    let start_bar = requested_start.min(bar_count - 1);
    let end_bar = requested_end.min(bar_count - 1).max(start_bar);
    let start_beat = start_bar as f64 * bar_beats;
    let end_beat = ((end_bar + 1) as f64 * bar_beats).min(exercise_length);
    (start_bar, end_bar, start_beat, end_beat)
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

fn expanded_range_tempo_map(
    exercise: &Exercise,
    start_beat: f64,
    end_beat: f64,
    loops: u8,
) -> Vec<TempoChange> {
    let length = (end_beat - start_beat).max(0.001);
    let start_bpm = exercise
        .tempo_map
        .iter()
        .take_while(|change| change.beat <= start_beat + 0.000_001)
        .last()
        .map(|change| change.bpm)
        .unwrap_or(exercise.tempo_bpm);
    let mut result = Vec::new();
    for repetition in 0..loops {
        let repetition_start = start_beat + repetition as f64 * length;
        result.push(TempoChange {
            tick: (repetition_start * exercise.ticks_per_beat as f64).round() as u64,
            beat: repetition_start,
            bpm: start_bpm,
        });
        result.extend(
            exercise
                .tempo_map
                .iter()
                .filter(|change| {
                    change.beat > start_beat + 0.000_001 && change.beat < end_beat - 0.000_001
                })
                .cloned()
                .map(|mut change| {
                    change.beat = repetition_start + change.beat - start_beat;
                    change.tick = (change.beat * exercise.ticks_per_beat as f64).round() as u64;
                    change
                }),
        );
    }
    result
}

fn retime_tempo_map(
    existing: &[TempoChange],
    exercise: &Exercise,
    loops: u8,
    beat: f64,
    bpm: f64,
) -> Vec<TempoChange> {
    let source = expanded_tempo_map(exercise, loops);
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

fn expanded_range_hits(
    exercise: &Exercise,
    start_beat: f64,
    end_beat: f64,
    loops: u8,
) -> Vec<ExpectedHit> {
    let length = (end_beat - start_beat).max(0.0);
    let source = hits_in_bar(&exercise.hits, start_beat, end_beat);
    let mut result = Vec::with_capacity(source.len() * loops as usize);
    for repetition in 0..loops {
        for hit in &source {
            let mut copy = hit.clone();
            copy.id = format!("{}-loop{}", copy.id, repetition + 1);
            copy.beat += repetition as f64 * length;
            result.push(copy);
        }
    }
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

struct LiveTimingContext<'a> {
    session_start_ms: f64,
    bpm: f64,
    tempo_map: &'a [TempoChange],
    range_start: f64,
    range_length: f64,
    latency_ms: f64,
    config: ScoringConfig,
}

fn display_beat_in_range(beat: f64, start: f64, length: f64) -> f64 {
    start + (beat - start).rem_euclid(length.max(0.001))
}

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
            expected_beat: Some(display_beat_in_range(
                hit.beat,
                context.range_start,
                context.range_length,
            )),
            actual_beat: Some(display_beat_in_range(
                actual_beat,
                context.range_start,
                context.range_length,
            )),
            instrument: observed.instrument,
            judgement,
            error_ms,
            excellent: judgement == Judgement::Perfect,
            note_score: context.config.note_score(error_ms),
            event_id: Some(format!("{:.3}", observed.timestamp_ms)),
        };
    }

    let wrong_instrument = expected.iter().any(|hit| {
        hit.instrument != observed.instrument
            && !matched_expected_ids.contains(&hit.id)
            && timing_error(hit).abs() <= match_ms
    });
    LiveJudgement {
        expected_id: None,
        expected_beat: None,
        actual_beat: Some(display_beat_in_range(
            actual_beat.max(context.range_start),
            context.range_start,
            context.range_length,
        )),
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
    let tempo_down = change_tempo.clone();
    let tempo_input = change_tempo.clone();
    let tempo_commit = change_tempo.clone();
    let tempo_up = change_tempo;
    let save_down = save.clone();
    let save_input = save.clone();
    let save_commit = save.clone();
    let save_up = save.clone();
    let save_enabled = save.clone();
    let save_count = save.clone();
    let save_subdivision = save.clone();
    let save_timbre = save.clone();
    view! {
      <label class="toggle-row metronome-enabled"><span>"Click during exercise"</span><input type="checkbox" prop:checked=move || metronome.get() on:change=move |event|{metronome.set(event_target_checked(&event));save_enabled();}/></label>
      <div class="tempo-field"><span>"Tempo (BPM)"</span>
        <div class="tempo-control">
          <button type="button" aria-label="Decrease tempo" on:click=move |_|{tempo_down(bpm.get_untracked()-1.0);save_down();}><Minus size=17/></button>
          <div class="tempo-value">
            <input aria-label="Tempo in BPM" title="Type a tempo from 30 to 240 BPM" type="number" inputmode="numeric" min="30" max="240" step="1"
              prop:value=move || bpm.get().round().to_string()
              on:focus=move |event|{event_target::<web_sys::HtmlInputElement>(&event).select();}
              on:input=move |event|{
                if let Ok(value)=event_target_value(&event).parse::<f64>() && (30.0..=240.0).contains(&value){
                  tempo_input(value.round());save_input();
                }
              }
              on:change=move |event|{
                let input=event_target::<web_sys::HtmlInputElement>(&event);
                let value=input.value().parse::<f64>().unwrap_or_else(|_|bpm.get_untracked()).round().clamp(30.0,240.0);
                tempo_commit(value);input.set_value(&value.to_string());save_commit();
              }/>
            <span>"BPM"</span>
          </div>
          <button type="button" aria-label="Increase tempo" on:click=move |_|{tempo_up(bpm.get_untracked()+1.0);save_up();}><Plus size=17/></button>
        </div>
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
    let loops = RwSignal::new(1u8);
    let repeat_bar_until_good = RwSignal::new(false);
    let repeat_tolerance = RwSignal::new(RepeatTolerance::Standard);
    let selection_start_bar = RwSignal::new(0usize);
    let selection_end_bar = RwSignal::new(usize::MAX);
    let context_popup = RwSignal::new(None::<ContextPopup>);
    let progress = RwSignal::new(-1.0);
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
    let live = RwSignal::new(Vec::<LiveJudgement>::new());
    let live_matched_expected = RwSignal::new(HashSet::<String>::new());
    let input_mode = RwSignal::new(InputSource::Demo);
    let result = RwSignal::new(None::<SessionRecord>);
    let history = RwSignal::new(Vec::<SessionRecord>::new());
    let comparison = RwSignal::new(None::<f64>);
    let settings_open = RwSignal::new(false);
    let stats_open = RwSignal::new(false);
    let library_open = RwSignal::new(false);
    let library_personal = RwSignal::new(false);
    let library_page = RwSignal::new(0usize);
    let result_modal_open = RwSignal::new(false);
    let result_dismiss_after_ms = RwSignal::new(f64::INFINITY);
    let welcome = RwSignal::new(true);
    let import_error = RwSignal::new(None::<String>);
    let metronome_preview_error = RwSignal::new(None::<String>);
    let midi_status = RwSignal::new("Not connected".to_string());
    let midi_ports = RwSignal::new(Vec::<MidiPort>::new());
    let selected_midi = RwSignal::new(None::<String>);
    let diagnostics = RwSignal::new(Vec::<String>::new());
    let midi_learn = RwSignal::new(false);
    let snare_note = RwSignal::new(38u8);
    let snare_channel = RwSignal::new(10u8);
    let latency_ms = RwSignal::new(0.0);
    let calibration_targets = RwSignal::new(Vec::<f64>::new());
    let calibration_errors = RwSignal::new(Vec::<f64>::new());
    let on_time_ms = RwSignal::new(DEFAULT_GOOD_MS);
    let excellent_ms = RwSignal::new(DEFAULT_PERFECT_MS);
    let runtime_tempo_map = RwSignal::new(scaled_tempo_map(&paradiddle(), 1, 60.0));

    let save_current_settings = Rc::new(move || {
        persist_settings(Settings {
            mode: Some(app_mode.get_untracked()),
            bpm: Some(bpm.get_untracked()),
            count_in_bars: Some(count_in.get_untracked()),
            subdivision: Some(subdivision.get_untracked()),
            timbre: Some(timbre.get_untracked()),
            metronome: Some(metronome.get_untracked()),
            click_volume: Some(click_volume.get_untracked()),
            snare_volume: Some(snare_volume.get_untracked()),
            snare_muted: Some(snare_muted.get_untracked()),
            snare_reference_mode: Some(snare_reference_mode.get_untracked()),
            loops: Some(loops.get_untracked()),
            repeat_bar_until_good: Some(repeat_bar_until_good.get_untracked()),
            repeat_tolerance: Some(repeat_tolerance.get_untracked()),
            selection_start_bar: Some(selection_start_bar.get_untracked()),
            selection_end_bar: Some(selection_end_bar.get_untracked()),
            latency_ms: Some(latency_ms.get_untracked()),
            on_time_ms: Some(on_time_ms.get_untracked()),
            excellent_ms: Some(excellent_ms.get_untracked()),
            snare_note: Some(snare_note.get_untracked()),
            snare_channel: Some(snare_channel.get_untracked()),
            welcome_dismissed: Some(!welcome.get_untracked()),
            last_import_hash: selected_import.get_untracked(),
        })
    });

    let finish_run: Rc<dyn Fn()> = Rc::new(move || {
        phase.set("finishing".into());
        let expected = run_expected.get_untracked();
        let tempo_map = runtime_tempo_map.get_untracked();
        let scoring_bpm = tempo_map
            .first()
            .map(|change| change.bpm)
            .unwrap_or(bpm.get_untracked());
        let mut summary = score_session_with_tempo_map(
            expected.as_ref(),
            &observed.get_untracked(),
            scoring_bpm,
            run_start_ms.get_untracked(),
            latency_ms.get_untracked(),
            ScoringConfig {
                perfect_ms: excellent_ms.get_untracked(),
                good_ms: on_time_ms.get_untracked(),
                ..ScoringConfig::default()
            },
            &tempo_map,
        );
        include_failed_repeat_attempts(&mut summary, &failed_repeat_attempts.get_untracked());
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
        result_dismiss_after_ms.set(
            web_sys::window()
                .and_then(|window| window.performance())
                .map(|performance| performance.now() + 2_000.0)
                .unwrap_or(Date::now() + 2_000.0),
        );
        result_modal_open.set(true);
        focus_result_close();
    });

    let frame_holder: CallbackSlot<FrameCallback> = Rc::new(RefCell::new(None));
    let done_holder: CallbackSlot<DoneCallback> = Rc::new(RefCell::new(None));
    let launch_holder: LaunchSlot = Rc::new(RefCell::new(None));
    let launch_run: LaunchRun = {
        let finish_run = finish_run.clone();
        let frame_holder = frame_holder.clone();
        let done_holder = done_holder.clone();
        let launch_holder = launch_holder.clone();
        Rc::new(move |offset, count_bars, immediate| {
            phase.set(if count_bars > 0 {
                "count_in".into()
            } else {
                "starting".into()
            });
            let current_exercise = exercise.get_untracked();
            let bar_beats = beats_per_bar(&current_exercise);
            let exercise_length = exercise_beats(&current_exercise);
            let (_, _, range_start, range_end) = selected_bar_range(
                &current_exercise,
                selection_start_bar.get_untracked(),
                selection_end_bar.get_untracked(),
            );
            let configured_loops = if loops.get_untracked() == 0 {
                CONTINUOUS_LOOP_CHUNK
            } else {
                loops.get_untracked()
            };
            let total = range_start + (range_end - range_start) * configured_loops as f64;
            let repeat_enabled = repeat_bar_until_good.get_untracked();
            let (segment_start, segment_end) = if repeat_enabled {
                repeat_bar_segment(offset, bar_beats, total)
            } else {
                (offset, total)
            };
            active_segment_start.set(segment_start);
            let tempo_map = expanded_range_tempo_map(
                &current_exercise,
                range_start,
                range_end,
                configured_loops,
            );
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
                reference_snare_notes(run_expected.get_untracked().as_ref(), offset, segment_end)
            } else {
                Vec::new()
            };
            let options=json_compatible(&serde_json::json!({"bpm":bpm.get_untracked(),"sourceBpm":current_exercise.tempo_bpm,"tempoMap":tempo_events,"countInBars":count_bars,"meter":bar_beats,"exerciseBeats":exercise_length,"displayRangeStart":range_start,"displayRangeBeats":range_end-range_start,"totalBeats":segment_end,"offsetBeat":offset,"immediate":immediate,"subdivision":subdivision.get_untracked(),"metronome":metronome.get_untracked(),"timbre":timbre.get_untracked(),"referenceNotes":reference_notes})).unwrap_or(JsValue::NULL);
            let origin_set = Rc::new(Cell::new(false));
            let range_length = (range_end - range_start).max(0.001);
            let initial_visible_loop =
                ((offset - range_start).max(0.0) / range_length).floor() as usize;
            visible_loop_index.set(initial_visible_loop);
            let frame = Closure::wrap(Box::new(move |beat: f64, _now: f64, start: f64| {
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
                    paused_run_start_ms.set(None);
                }
                run_start_ms.set(session_start);
                progress.set(beat);
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
                if !repeat_enabled && beat >= range_start {
                    let current_visible_loop =
                        ((beat - range_start) / range_length).floor() as usize;
                    if current_visible_loop > visible_loop_index.get_untracked()
                        && beat < total - 0.000_001
                    {
                        visible_loop_index.set(current_visible_loop);
                        live.set(vec![]);
                        feedback_dirty.set(false);
                        clear_score_feedback_preserve_viewport();
                    }
                }
                if beat >= offset - 0.000_001 && feedback_dirty.get_untracked() {
                    feedback_dirty.set(false);
                    let visible = live.get_untracked();
                    let judgement_values =
                        serde_wasm_bindgen::to_value(&visible).unwrap_or(JsValue::NULL);
                    let display_beat = range_start
                        + (beat - range_start).rem_euclid((range_end - range_start).max(0.001));
                    update_score(display_beat, judgement_values);
                }
            }) as Box<dyn FnMut(f64, f64, f64)>);
            let done_finish = finish_run.clone();
            let repeat_launch = launch_holder.clone();
            let done = Closure::wrap(Box::new(move || {
                if !repeat_enabled {
                    if app_mode.get_untracked() == AppMode::Practice && loops.get_untracked() == 0 {
                        observed.set(vec![]);
                        live.set(vec![]);
                        live_matched_expected.set(HashSet::new());
                        feedback_dirty.set(true);
                        clear_score_feedback_preserve_viewport();
                        let next = repeat_launch.borrow().as_ref().cloned();
                        if let Some(launch) = next {
                            spawn_local(async move { launch(range_start, 0, true) });
                        }
                        return;
                    }
                    done_finish();
                    return;
                }
                let all_expected = run_expected.get_untracked();
                let expected = hits_in_bar(all_expected.as_ref(), segment_start, segment_end);
                let attempt_origin = run_start_ms.get_untracked();
                if bar_attempt_is_good(
                    expected.len(),
                    live_matched_expected.get_untracked().len(),
                    &live.get_untracked(),
                    repeat_tolerance.get_untracked(),
                ) {
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
                            spawn_local(async move { launch(segment_end, 0, true) });
                        }
                    }
                } else {
                    failed_repeat_attempts
                        .update(|attempts| attempts.push((expected.len(), live.get_untracked())));
                    observed.set(vec![]);
                    feedback_dirty.set(true);
                    live.set(vec![]);
                    live_matched_expected.set(HashSet::new());
                    bar_attempt.update(|attempt| *attempt += 1);
                    clear_score_feedback_preserve_viewport();
                    let next = repeat_launch.borrow().as_ref().cloned();
                    if let Some(launch) = next {
                        spawn_local(async move { launch(segment_start, 0, true) });
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
        progress.set(current_audio_beat().max(0.0));
        paused_run_start_ms.set(Some(run_start_ms.get_untracked()));
        stop_audio();
        phase.set("paused".into());
    });
    let resume_run: Rc<dyn Fn()> = {
        let launch = launch_run.clone();
        Rc::new(move || {
            if phase.get_untracked() == "paused" {
                launch(progress.get_untracked().max(0.0), 0, true);
            }
        })
    };

    let reset_run: Rc<dyn Fn()> = Rc::new(move || {
        stop_audio();
        result_modal_open.set(false);
        observed.set(vec![]);
        feedback_dirty.set(true);
        accepted_observed.set(vec![]);
        practice_origin_ms.set(0.0);
        paused_run_start_ms.set(None);
        bar_attempt.set(1);
        failed_repeat_attempts.set(vec![]);
        live.set(vec![]);
        live_matched_expected.set(HashSet::new());
        result.set(None);
        progress.set(-1.0);
        clear_score_feedback();
        phase.set("idle".into());
    });

    let stop_for_navigation: Rc<dyn Fn()> = Rc::new(move || {
        if matches!(
            phase.get_untracked().as_str(),
            "running" | "count_in" | "paused"
        ) {
            stop_audio();
            observed.set(vec![]);
            feedback_dirty.set(true);
            accepted_observed.set(vec![]);
            practice_origin_ms.set(0.0);
            run_start_ms.set(0.0);
            paused_run_start_ms.set(None);
            bar_attempt.set(1);
            failed_repeat_attempts.set(vec![]);
            live.set(vec![]);
            live_matched_expected.set(HashSet::new());
            result.set(None);
            progress.set(-1.0);
            clear_score_feedback();
            phase.set("idle".into());
        }
    });

    let change_tempo: Rc<dyn Fn(f64)> = Rc::new(move |value| {
        let value = value.clamp(30.0, 240.0);
        bpm.set(value);
        if matches!(
            phase.get_untracked().as_str(),
            "running" | "count_in" | "paused"
        ) {
            let current_exercise = exercise.get_untracked();
            let beat = progress.get_untracked().max(0.0);
            runtime_tempo_map.set(retime_tempo_map(
                &runtime_tempo_map.get_untracked(),
                &current_exercise,
                loops.get_untracked(),
                beat,
                value,
            ));
            set_audio_tempo(value);
        }
    });

    let start_new_run: Rc<dyn Fn(u8)> = {
        let launch = launch_run.clone();
        Rc::new(move |count_bars| {
            result_modal_open.set(false);
            let current_exercise = exercise.get_untracked();
            let bar_count = (exercise_beats(&current_exercise) / beats_per_bar(&current_exercise))
                .ceil()
                .max(1.0) as usize;
            let (start_bar, end_bar, start_beat, end_beat) = selected_bar_range(
                &current_exercise,
                selection_start_bar.get_untracked(),
                selection_end_bar.get_untracked(),
            );
            app_mode.set(
                if start_bar == 0 && end_bar + 1 == bar_count && loops.get_untracked() == 1 {
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
            run_expected.set(Arc::new(expanded_range_hits(
                &current_exercise,
                start_beat,
                end_beat,
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
            live.set(vec![]);
            live_matched_expected.set(HashSet::new());
            result.set(None);
            progress.set(-1.0);
            clear_score_feedback();
            scroll_score_to_beat(start_beat);
            launch(start_beat, count_bars, false);
        })
    };

    let save_for_hit = save_current_settings.clone();
    let start_for_hit = start_new_run.clone();
    let hit_input: HitInput = Rc::new(
        move |timestamp, instrument, velocity, source, raw_note, raw_channel| {
            if result_modal_open.get_untracked() {
                if timestamp >= result_dismiss_after_ms.get_untracked() {
                    result_modal_open.set(false);
                    result_dismiss_after_ms.set(f64::INFINITY);
                    clear_score_feedback();
                    restore_result_focus();
                }
                return;
            }
            if !snare_muted.get_untracked() && snare_reference_mode.get_untracked() == "hits" {
                play_snare(velocity);
            }
            input_mode.set(source);
            if library_open.get_untracked() {
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
                let current_exercise = exercise.get_untracked();
                let first_beat = selected_bar_range(
                    &current_exercise,
                    selection_start_bar.get_untracked(),
                    selection_end_bar.get_untracked(),
                )
                .2;
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
                velocity,
                source,
                raw_note,
                raw_channel,
            };
            let current_exercise = exercise.get_untracked();
            let all_expected = run_expected.get_untracked();
            let expected_in_bar;
            let expected = if repeat_bar_until_good.get_untracked() {
                let bar_beats = beats_per_bar(&current_exercise);
                expected_in_bar = hits_for_bar_attempt(
                    all_expected.as_ref(),
                    active_segment_start.get_untracked(),
                    bar_beats,
                );
                expected_in_bar.as_slice()
            } else {
                all_expected.as_slice()
            };
            let session_start = run_start_ms.get_untracked();
            let tempo_map = runtime_tempo_map.get_untracked();
            let scoring_bpm = tempo_map
                .first()
                .map(|change| change.bpm)
                .unwrap_or(bpm.get_untracked());
            let config = ScoringConfig {
                perfect_ms: excellent_ms.get_untracked(),
                good_ms: on_time_ms.get_untracked(),
                ..ScoringConfig::default()
            };
            let (_, _, range_start, range_end) = selected_bar_range(
                &current_exercise,
                selection_start_bar.get_untracked(),
                selection_end_bar.get_untracked(),
            );
            let mut matched_expected = live_matched_expected.get_untracked();
            let previously_matched = matched_expected.clone();
            let timing = LiveTimingContext {
                session_start_ms: session_start,
                bpm: scoring_bpm,
                tempo_map: &tempo_map,
                range_start,
                range_length: range_end - range_start,
                latency_ms: latency_ms.get_untracked(),
                config,
            };
            let judgement =
                live_judgement_for_hit(expected, &mut matched_expected, &event, &timing);
            if !repeat_bar_until_good.get_untracked() {
                let matched_beat = expected
                    .iter()
                    .find(|hit| {
                        !previously_matched.contains(&hit.id) && matched_expected.contains(&hit.id)
                    })
                    .map(|hit| hit.beat);
                let absolute_beat = matched_beat.unwrap_or_else(|| {
                    beat_at_elapsed_ms(
                        timestamp - timing.latency_ms - timing.session_start_ms,
                        timing.bpm,
                        timing.tempo_map,
                    )
                });
                let loop_index = ((absolute_beat - range_start).max(0.0)
                    / (range_end - range_start).max(0.001))
                .floor() as usize;
                if loop_index > visible_loop_index.get_untracked() {
                    visible_loop_index.set(loop_index);
                    live.set(vec![]);
                    clear_score_feedback_preserve_viewport();
                }
            }
            observed.update(|items| items.push(event.clone()));
            feedback_dirty.set(true);
            live_matched_expected.set(matched_expected);
            if let Ok(value) = json_compatible(&judgement) {
                let display_beat = range_start
                    + (progress.get_untracked() - range_start)
                        .rem_euclid((range_end - range_start).max(0.001));
                append_score_judgement(display_beat, value);
            }
            live.update(|judgements| judgements.push(judgement));
        },
    );

    let file_callback: Rc<MidiFileCallback> = Rc::new(Closure::wrap(Box::new(
        move |bytes: Uint8Array, name: String, error: String, save_to_library: JsValue| {
            if library_open.get_untracked() {
                library_open.set(false);
                restore_library_focus();
            }
            if matches!(
                phase.get_untracked().as_str(),
                "running" | "count_in" | "paused"
            ) {
                stop_audio();
                observed.set(vec![]);
                accepted_observed.set(vec![]);
                practice_origin_ms.set(0.0);
                paused_run_start_ms.set(None);
                bar_attempt.set(1);
                failed_repeat_attempts.set(vec![]);
                live.set(vec![]);
                live_matched_expected.set(HashSet::new());
                progress.set(-1.0);
                clear_score_feedback();
                phase.set("idle".into());
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
                if let Some(error) = event.error {
                    midi_status.set(format!("MIDI error · {error}"));
                } else {
                    if event.disconnected_id.is_some()
                        && matches!(phase.get_untracked().as_str(), "running" | "count_in")
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
                    } else if event.permission.as_deref() == Some("prompt") {
                        midi_status.set("MIDI ready · connect to grant access".into());
                    } else if event.permission.as_deref() == Some("unsupported") {
                        midi_status.set("Web MIDI unavailable in this browser".into());
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
            if velocity == 0 {
                return;
            }
            if midi_learn.get_untracked() {
                snare_note.set(note);
                snare_channel.set(channel);
                midi_learn.set(false);
                midi_status.set(format!("Snare learned · note {note}, channel {channel}"));
                let profile = serde_json::json!({"id":event.input_id.unwrap_or_else(||"user-device".into()),"name":event.input_name.unwrap_or_else(||"MIDI drums".into()),"schemaVersion":1,"verified":true,"bindings":[{"note":note,"channel":channel,"instrument":"snare"}],"latencyMs":latency_ms.get_untracked()});
                if let Ok(value) = json_compatible(&profile) {
                    spawn_local(async move {
                        let _ = JsFuture::from(save_profile(value)).await;
                    });
                }
                save_for_midi();
                return;
            }
            let instrument =
                if note == snare_note.get_untracked() && channel == snare_channel.get_untracked() {
                    Instrument::Snare
                } else {
                    Instrument::from_gm(note)
                };
            midi_hit(
                event.timestamp.unwrap_or_else(|| {
                    web_sys::window()
                        .and_then(|window| window.performance())
                        .map(|performance| performance.now())
                        .unwrap_or(0.0)
                }),
                instrument,
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
            bind_drop(callback);
        });
    }
    Effect::new(move |_| {
        let current_exercise = exercise.get();
        let (start, end, _, _) = selected_bar_range(
            &current_exercise,
            selection_start_bar.get(),
            selection_end_bar.get(),
        );
        set_selected_bars(start, end);
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
        if let Ok(value) = JsFuture::from(load_sessions()).await
            && let Ok(items) = serde_wasm_bindgen::from_value::<Vec<SessionRecord>>(value)
        {
            history.set(items);
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
                    if let Some(v) = saved.selection_start_bar {
                        selection_start_bar.set(v);
                    }
                    if let Some(v) = saved.selection_end_bar {
                        selection_end_bar.set(v);
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
                    welcome.set(!saved.welcome_dismissed.unwrap_or(false));
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
                    import_error.set(Some(format!("Saved settings could not load: {error}")))
                }
            },
            Err(error) => {
                import_error.set(Some(format!("Saved settings storage failed: {error:?}")))
            }
        }
    });

    let key_reset_run = reset_run.clone();
    let key_start_new_run = start_new_run.clone();
    let key_hit = hit_input.clone();
    let key_pause_run = pause_run.clone();
    let key_resume_run = resume_run.clone();
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
        if library_open.get_untracked() {
            if event.key() == "Escape" {
                event.prevent_default();
                library_open.set(false);
                restore_library_focus();
            } else if matches!(event.key().to_lowercase().as_str(), "j" | " " | "r") {
                event.prevent_default();
            }
            return;
        }
        if stats_open.get_untracked() {
            match event.key().to_lowercase().as_str() {
                "escape" => {
                    event.prevent_default();
                    stats_open.set(false);
                    restore_stats_focus();
                    return;
                }
                "j" => {}
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
                    result_modal_open.set(false);
                    result_dismiss_after_ms.set(f64::INFINITY);
                    clear_score_feedback();
                    restore_result_focus();
                    return;
                }
                "j" => {}
                " " | "r" => {
                    event.prevent_default();
                    return;
                }
                _ => return,
            }
        }
        let target = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::HtmlElement>().ok());
        if event.key() == " "
            && target.as_ref().is_some_and(|element| {
                element.tag_name() == "BUTTON"
                    && element
                        .closest(".bar-selection-control, .metronome-context")
                        .ok()
                        .flatten()
                        .is_some()
            })
        {
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
                    108,
                    InputSource::Demo,
                    None,
                    None,
                );
            }
            " " => {
                event.prevent_default();
                match phase.get_untracked().as_str() {
                    "running" | "count_in" => key_pause_run(),
                    "paused" => key_resume_run(),
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
        if web_sys::window()
            .and_then(|window| window.document())
            .map(|document| document.visibility_state() == web_sys::VisibilityState::Hidden)
            .unwrap_or(false)
            && matches!(phase.get_untracked().as_str(), "running" | "count_in")
        {
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
    let stop_for_connect = stop_for_navigation.clone();
    let stop_for_stats = stop_for_navigation.clone();
    let stop_for_settings = stop_for_navigation.clone();
    let stop_for_import = stop_for_navigation.clone();
    let stop_for_library = stop_for_navigation.clone();
    let save_welcome = save_current_settings.clone();
    let save_snare_mute = save_current_settings.clone();
    let save_snare_reference_mode = save_current_settings.clone();
    let save_snare_volume = save_current_settings.clone();
    let save_loops = save_current_settings.clone();
    let save_repeat_bar = save_current_settings.clone();
    let save_repeat_tolerance = save_current_settings.clone();
    let save_selection_start = save_current_settings.clone();
    let save_selection_end = save_current_settings.clone();
    let save_selection_all = save_current_settings.clone();
    let save_cal_reset = save_current_settings.clone();
    let save_cal_manual = save_current_settings.clone();
    let save_on_time = save_current_settings.clone();
    let save_excellent = save_current_settings.clone();
    view! {
      <main class="app">
        <header class="topbar">
          <button id="midi-library-trigger" class="stats-trigger library-topbar-trigger" aria-label=move || format!("MIDI library · {} exercises",saved_imports.get().len()+BUILTIN_RUDIMENTS.len()) title=move || format!("MIDI library · {} exercises",saved_imports.get().len()+BUILTIN_RUDIMENTS.len()) aria-haspopup="dialog" aria-expanded=move || library_open.get().to_string() on:click=move |_|{
            stop_for_library();
            library_personal.set(false);
            library_page.set(0);
            library_open.set(true);
            focus_library_close();
          }><FolderOpen size=19/><span class="library-count" aria-hidden="true">{move || saved_imports.get().len()+BUILTIN_RUDIMENTS.len()}</span></button>
          <button id="stats-trigger" class="stats-trigger" aria-label="Stats" title="Performance history" aria-haspopup="dialog" aria-expanded=move || stats_open.get().to_string() on:click=move |_|{stop_for_stats();stats_open.set(true);focus_stats_close();}><ChartNoAxesCombined size=19/></button>
          <a class="brand" href="#" aria-label="Rudiment"><span class="brand-mark" aria-hidden="true"></span><span>"Rudiment"</span></a>
          <div class="exercise-heading"><span class="eyebrow">"CURRENT EXERCISE"</span><strong>{move || exercise.get().title}</strong><small>"4/4 · sticking and accents"</small></div>
          <button id="settings-trigger" class="stats-trigger settings-trigger" aria-label="Open settings" title="Rudiment settings" aria-haspopup="dialog" aria-expanded=move || settings_open.get().to_string() on:click=move |_|{stop_for_settings();settings_open.set(true);focus_settings_close();}><SettingsIcon size=19/></button>
          <button class=move || if selected_midi.get().is_some(){"status-pill connected"}else{"status-pill"} aria-label=move || format!("MIDI connection: {}",midi_status.get()) on:click={let connect=connect_action.clone();move |_|{stop_for_connect();connect();}}><span class="status-dot"></span>{move || midi_status.get()}</button>
        </header>

          <section class="workspace">
              <aside class=move || if welcome.get(){"welcome"}else{"welcome hidden"}><div><strong>              "A paradiddle is ready."</strong><span>"Space pauses; R restarts."</span></div><button aria-label="Got it, dismiss welcome hint" on:click=move |_|{welcome.set(false);save_welcome();}>"Got it"</button></aside>
            <section id="score-card" class="score-card">
              <div class="score-meta">
                <div class="metronome-context">
                  <button id="visual-metronome" class="visual-metronome" type="button" aria-label="Metronome settings, READY" aria-haspopup="dialog" aria-controls="metronome-popover" aria-expanded=move || (context_popup.get()==Some(ContextPopup::Metronome)).to_string() on:click=move |_|context_popup.update(|popup|*popup=if *popup==Some(ContextPopup::Metronome){None}else{Some(ContextPopup::Metronome)})>
                    <span class="visual-metronome-sweep" aria-hidden="true"><i></i></span>
                    <span class="visual-metronome-beats" aria-hidden="true">{move || (0..beats_per_bar(&exercise.get()).round().max(1.0) as usize).map(|_|view!{<i></i>}).collect_view()}</span>
                    <small class="visual-metronome-label" aria-hidden="true">"READY"</small>
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
              <div class=move || if import_error.get().is_some(){"error-row"}else{"error-row hidden"}>{move || import_error.get().unwrap_or_default()}</div>
              <div class="position-row">
                <div class=move || if context_popup.get()==Some(ContextPopup::Bars){"bar-selection-control open"}else{"bar-selection-control"}>
                  <button id="bar-selection-trigger" class="bar-selection-trigger" type="button" aria-labelledby="bar-selection-label bar-selection-current" aria-haspopup="dialog" aria-controls="bar-selection-popover" aria-expanded=move || (context_popup.get()==Some(ContextPopup::Bars)).to_string() on:click=move |_|context_popup.update(|popup|*popup=if *popup==Some(ContextPopup::Bars){None}else{Some(ContextPopup::Bars)})><span id="bar-selection-label" class="sr-only">"Select bars"</span><span id="bar-selection-current">{move || {let bar_beats=beats_per_bar(&exercise.get());let total=(exercise_beats(&exercise.get())/bar_beats).ceil().max(1.0) as usize;let raw=(progress.get().max(0.0)/bar_beats).floor() as usize;let start=selection_start_bar.get().min(total.saturating_sub(1));let end=selection_end_bar.get().min(total.saturating_sub(1)).max(start);let full_performance=start==0&&end+1==total&&app_mode.get()==AppMode::Performance;let current=if full_performance{raw.min(total.saturating_sub(1))+1}else{start+raw.saturating_sub(start)%(end-start+1)+1};let range_total=end+1;if repeat_bar_until_good.get(){format!("BAR {current} / {range_total} · TRY {}",bar_attempt.get())}else{format!("BAR {current} / {range_total}")}}}</span></button>
                  <div id="bar-selection-popover" class=move || if context_popup.get()==Some(ContextPopup::Bars){"bar-selection-popover"}else{"bar-selection-popover hidden"} role="dialog" aria-label="Bar selection">
                    <div class="bar-selection-popover-head"><strong>"Practise bars"</strong><button class="bar-selection-close" type="button" aria-label="Close bar selection" on:click=move |_|{context_popup.set(None);focus_popup_trigger(ContextPopup::Bars);}><X size=15/></button></div>
                    <div class="bar-range-fields">
                      <label>"From"<select aria-label="First selected bar" prop:value=move || selection_start_bar.get().to_string() on:change={let stop=stop_for_navigation.clone();move |event|{stop();let value=event_target_value(&event).parse().unwrap_or(0);selection_start_bar.set(value);if selection_end_bar.get_untracked()<value{selection_end_bar.set(value);}save_selection_start();}}>{move || {let count=(exercise_beats(&exercise.get())/beats_per_bar(&exercise.get())).ceil().max(1.0) as usize;(0..count).map(|bar|view!{<option value=bar.to_string()>{format!("Bar {}",bar+1)}</option>}).collect_view()}}</select></label>
                      <label>"To"<select aria-label="Last selected bar" prop:value=move || selection_end_bar.get().min((exercise_beats(&exercise.get())/beats_per_bar(&exercise.get())).ceil().max(1.0) as usize-1).to_string() on:change={let stop=stop_for_navigation.clone();move |event|{stop();let value=event_target_value(&event).parse().unwrap_or(selection_start_bar.get_untracked());selection_end_bar.set(value.max(selection_start_bar.get_untracked()));save_selection_end();}}>{move || {let count=(exercise_beats(&exercise.get())/beats_per_bar(&exercise.get())).ceil().max(1.0) as usize;(0..count).map(|bar|view!{<option value=bar.to_string()>{format!("Bar {}",bar+1)}</option>}).collect_view()}}</select></label>
                    </div>
                    <button class="select-all-bars" type="button" on:click={let stop=stop_for_navigation.clone();move |_|{stop();selection_start_bar.set(0);selection_end_bar.set(usize::MAX);save_selection_all();}}>"All bars"</button>
                    <small>{move || {let count=(exercise_beats(&exercise.get())/beats_per_bar(&exercise.get())).ceil().max(1.0) as usize;let start=selection_start_bar.get().min(count-1);let end=selection_end_bar.get().min(count-1).max(start);if start==0&&end+1==count{"All bars · Performance when played once".into()}else if start==end{format!("Bar {} selected · Practice",start+1)}else{format!("Bars {}–{} selected · Practice",start+1,end+1)}}}</small>
                  </div>
                </div>
                <div class="timeline"><i style=move || format!("width:{:.2}%",(progress.get().max(0.0)/exercise_beats(&exercise.get())*100.0).rem_euclid(100.0).clamp(0.0,100.0))></i></div>
                <select class="loop-control" aria-label="Loops" title="Choose how many times to play the selected bars" prop:value=move || loops.get().to_string() on:change={let save=save_loops.clone();let stop=stop_for_navigation.clone();move |event|{stop();loops.set(event_target_value(&event).parse().unwrap_or(1));save();}}><option value="1">"1 / 1 LOOP"</option><option value="2">"2 LOOPS"</option><option value="4">"4 LOOPS"</option><option value="0">"∞ LOOP"</option></select>
              </div>
              <div class=move || if result_modal_open.get(){"result-backdrop"}else{"result-backdrop hidden"} on:click=move |_|{result_modal_open.set(false);result_dismiss_after_ms.set(f64::INFINITY);clear_score_feedback();restore_result_focus();}>
              <section id="result-dialog" class="stats-modal results-modal" role="dialog" aria-modal="true" aria-labelledby="result-title" on:click=|event|event.stop_propagation() on:keydown=move |event:web_sys::KeyboardEvent|{
                  match event.key().as_str(){
                    "Escape"=>{event.prevent_default();result_modal_open.set(false);result_dismiss_after_ms.set(f64::INFINITY);clear_score_feedback();restore_result_focus();},
                    "Tab"=>trap_dialog_tab(&event,"#result-dialog"),
                    _=>{},
                  }
                }>
                <header class="summary-header">
                  <div class="summary-title-group"><span class="summary-icon" aria-hidden="true">"R"</span><div><h2 id="result-title">"Session Complete"</h2><p>{move || result.get().map(|session|format!("{} · {:.0} BPM",session.exercise_title,session.bpm)).unwrap_or_default()}</p></div></div>
                  <button id="result-modal-close" aria-label="Close session results" on:click=move |_|{result_modal_open.set(false);result_dismiss_after_ms.set(f64::INFINITY);clear_score_feedback();restore_result_focus();}><X size=21/></button>
                </header>
                <section class="summary-achievement">
                  <div class=move || if result.get().is_some_and(|session|is_white_mastery(&session.summary)){"summary-stars mastery"}else{"summary-stars"} aria-label=move || result.get().map(|session|{let rating=performance_rating(&session.summary);if is_white_mastery(&session.summary){"Mastery, three white stars".into()}else{format!("{} of 3 gold stars",rating.stars)}}).unwrap_or_default()>{move || {let rating=result.get().map(|session|performance_rating(&session.summary)).unwrap_or(PerformanceRating{stars:0,award:PerformanceAward::None});(0..3).map(|index|view!{<Star size=38 fill=if index<rating.stars{"currentColor"}else{"none"}/>}).collect_view()}}</div>
                  <h3>{move || result.get().map(|session|achievement_copy(&session.summary).0).unwrap_or("")}</h3>
                  <p>{move || result.get().map(|session|achievement_copy(&session.summary).1).unwrap_or("")}</p>
                </section>
                <div class="summary-cards">
                  <section class="summary-card score-summary-card">
                    <h3>"Session Score"</h3>
                    <div class="summary-score"><strong>{move || display_score(result.get().and_then(|session|session.summary.score))}</strong><span>"/ 100"</span></div>
                    <span class=move || if result.get().is_some_and(|session|is_new_high_score(&session,&history.get())){"new-high-score"}else{"new-high-score hidden"}>"↗ New high score"</span>
                  </section>
                  <section class="summary-card breakdown-card">
                    <h3>"Hit Breakdown"</h3>
                    <div class="breakdown-list">{move || result.get().map(|session|{let (perfect,good,early,late)=timing_hit_counts(&session.summary);[("Perfect","perfect",perfect),("Good","good",good),("Early","early",early),("Late","late",late),("Missed","missed",session.summary.missed)].into_iter().map(|(label,class,count)|{let value=percentage(count,session.summary.expected);view!{<article class=format!("breakdown-row {class}")><span><i></i>{label}</span><strong>{format!("{count} / {}",session.summary.expected)}</strong><em><i style=format!("width:{value:.3}%")></i></em><b>{format!("{value:.0}%")}</b></article>}}).collect_view()}).unwrap_or_default()}</div>
                  </section>
                </div>
                <section class="summary-card progress-card">
                  <h3>"Your Progress"</h3><p>"Session score over time"</p>
                  {move || result.get().map(|session|{let scores=progress_scores(&session,&history.get());let points=progress_chart_points(&scores);let line=points.iter().map(|(x,y)|format!("{x:.1},{y:.1}")).collect::<Vec<_>>().join(" ");let latest=points.last().copied().zip(scores.last().copied());view!{
                    <svg class="progress-chart" viewBox="0 0 760 190" role="img" aria-label=format!("Scores for the last {} sessions",scores.len())>
                      <title>{format!("Last {} scores for this exercise at {:.0} BPM",scores.len(),session.bpm)}</title>
                      {[100_u8,75,50,25,0].into_iter().map(|value|{let y=146.0-132.0*value as f64/100.0;view!{<g class="progress-grid"><line x1="48" y1=y.to_string() x2="712" y2=y.to_string()/><text x="36" y=(y+4.0).to_string()>{value}</text></g>}}).collect_view()}
                      <polyline class="progress-line" points=line/>
                      {points.iter().enumerate().map(|(index,(x,y))|view!{<g><circle class=if index+1==points.len(){"progress-point latest"}else{"progress-point"} cx=x.to_string() cy=y.to_string() r=if index+1==points.len(){"5"}else{"3"}/><text class="progress-session-label" x=x.to_string() y="170">{index+1}</text></g>}).collect_view()}
                      {latest.map(|((x,y),score)|{let label_x=(x-22.0).clamp(4.0,712.0);let label_y=(y-31.0).max(1.0);view!{<g class="progress-latest-label"><rect x=label_x.to_string() y=label_y.to_string() width="44" height="22" rx="6"/><text x=(label_x+22.0).to_string() y=(label_y+15.0).to_string()>{format!("{score:.0}")}</text></g>}})}
                      <text class="progress-axis-label" x="380" y="188">"Session"</text>
                    </svg>
                  }.into_any()}).unwrap_or_else(||().into_any())}
                </section>
                <footer class="summary-footer"><p>{move || result.get().map(|session|format!("{} extra · {} wrong drum{}",session.summary.extras,session.summary.wrong_instrument,if session.summary.wrong_instrument==1{""}else{"s"})).unwrap_or_default()}</p><div class="result-actions"><button on:click=move |_|{result_modal_open.set(false);result_dismiss_after_ms.set(f64::INFINITY);clear_score_feedback();restore_result_focus();}>"Back to score"</button><button class="primary" on:click={let start=start_new_run.clone();move |_|{result_modal_open.set(false);result_dismiss_after_ms.set(f64::INFINITY);observed.set(vec![]);live.set(vec![]);result.set(None);progress.set(-1.0);start(count_in.get_untracked());}}>"Perform again"</button></div></footer>
                </section>
              </div>
            </section>
            <section class="practice-controls">
              <div class="transport">
                <button class="restart-button" on:click={let reset=reset_run.clone();move |_|reset()}><span class="transport-action-label"><span class="transport-icon" aria-hidden="true"><RotateCcw size=24/></span><span>"Restart"</span></span><small>"Clear current run"</small></button>
                <button class="primary start-button" aria-label=move || match phase.get().as_str(){"running"|"count_in"=>String::from("Pause"),"paused"=>String::from("Resume"),_=>{let count=(exercise_beats(&exercise.get())/beats_per_bar(&exercise.get())).ceil().max(1.0) as usize;if selection_start_bar.get()==0&&selection_end_bar.get().min(count-1)+1==count&&loops.get()==1{String::from("Start performance Hit your drum or press J to start Start practice")}else{String::from("Start practice Hit your drum or press J to start Start performance")}}} on:click={let start=start_new_run.clone();let pause=button_pause.clone();let resume=button_resume.clone();move |_|match phase.get_untracked().as_str(){"running"|"count_in"=>pause(),"paused"=>resume(),_=>{observed.set(vec![]);live.set(vec![]);result.set(None);input_mode.set(if selected_midi.get_untracked().is_some(){InputSource::Midi}else{InputSource::Demo});start(count_in.get_untracked());}}}><span class="transport-action-label"><span class="transport-icon" aria-hidden="true">{move || if matches!(phase.get().as_str(),"running"|"count_in"){view!{<Pause size=21/>}.into_any()}else{view!{<Play size=21 fill="currentColor"/>}.into_any()}}</span><span>{move || match phase.get().as_str(){"running"|"count_in"=>"Pause","paused"=>"Resume",_=>{let count=(exercise_beats(&exercise.get())/beats_per_bar(&exercise.get())).ceil().max(1.0) as usize;if selection_start_bar.get()==0&&selection_end_bar.get().min(count-1)+1==count&&loops.get()==1{"Start performance "}else{"Start practice "}}}}</span></span><small class=move || if phase.get()=="idle"{"start-instruction"}else{"start-instruction hidden"}>"Hit your drum or press J to start"</small></button>
                <aside class="timing-key" aria-label="Timing feedback key">
                  <span class="perfect"><i aria-hidden="true"></i>"Perfect"</span>
                  <span class="good"><i aria-hidden="true"></i>"Good"</span>
                  <span class="early"><i aria-hidden="true"></i>"Early"</span>
                  <span class="late"><i aria-hidden="true"></i>"Late"</span>
                  <span class="unmatched"><i aria-hidden="true"></i>"Missed"</span>
                </aside>
              </div>
            </section>
          </section>

            <div class=move || if library_open.get(){"stats-backdrop library-backdrop"}else{"stats-backdrop library-backdrop hidden"} on:click=move |_|{library_open.set(false);restore_library_focus();}>
              <section id="midi-library-dialog" class="stats-modal library-modal" role="dialog" aria-modal="true" aria-labelledby="midi-library-title" on:click=|event|event.stop_propagation() on:keydown=move |event:web_sys::KeyboardEvent|{
                match event.key().as_str(){
                  "Escape"=>{event.prevent_default();event.stop_propagation();library_open.set(false);restore_library_focus();},
                  "Tab"=>trap_dialog_tab(&event,"#midi-library-dialog"),
                  "j"|"J"|"r"|"R"|" "=>{event.prevent_default();event.stop_propagation();},
                  _=>{},
                }
              }>
                <div class="stats-modal-header library-modal-header">
                  <div><span class="eyebrow">"EXERCISES"</span><h2 id="midi-library-title">"MIDI library"</h2><p>{move || format!("{} built-in · {} personal",BUILTIN_RUDIMENTS.len(),saved_imports.get().len())}</p></div>
                  <button id="midi-library-close" aria-label="Close MIDI library" on:click=move |_|{library_open.set(false);restore_library_focus();}><X size=20/></button>
                </div>
                <div class="library-actions">
                  <button class="primary" on:click={let callback=choose_cb.clone();let stop=stop_for_import.clone();move |_|{
                    stop();
                    choose_midi(callback.as_ref().as_ref().unchecked_ref());
                  }}>"Import MIDI"</button>
                </div>
                <div class="library-tabs" role="tablist" aria-label="MIDI library sections">
                  <button role="tab" aria-selected=move || (!library_personal.get()).to_string() class=move || if library_personal.get(){""}else{"active"} on:click=move |_|{library_personal.set(false);library_page.set(0);}>{format!("Built-in · {}",BUILTIN_RUDIMENTS.len())}</button>
                  <button role="tab" aria-selected=move || library_personal.get().to_string() class=move || if library_personal.get(){"active"}else{""} on:click=move |_|{library_personal.set(true);library_page.set(0);}>{move || format!("Your imports · {}",saved_imports.get().len())}</button>
                </div>
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
                            <button class="library-delete" aria-label=format!("Delete {delete_name}") on:click=move |_|{
                              let confirmed=web_sys::window().and_then(|window|window.confirm_with_message(&format!("Delete {} from the local MIDI library?",delete_name)).ok()).unwrap_or(false);
                              if confirmed{
                                let hash=delete_hash.clone();
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
                              }
                            }>"Delete"</button>
                          </article>
                        }
                      }).collect_view().into_any()
                    }
                  }else{
                    let page=library_page.get().min(BUILTIN_RUDIMENTS.len().div_ceil(LIBRARY_PAGE_SIZE)-1);
                    BUILTIN_RUDIMENTS.into_iter().skip(page*LIBRARY_PAGE_SIZE).take(LIBRARY_PAGE_SIZE).map(|item|{
                      let midi_url=format!("public/exercises/rudiments/{}.mid",item.id);
                      let load_name=format!("{}.mid",item.title);
                      view!{
                        <article class="library-card bundled-entry" role="listitem">
                          <button class="library-load" on:click=move |_|{
                            library_open.set(false);
                            restore_library_focus();
                            load_bundled_from_library(&midi_url,&load_name);
                          }><strong>{item.title}</strong><small>{item.description}</small></button>
                        </article>
                      }
                    }).collect_view().into_any()
                  }}
                </div>
                <div class="library-pagination" aria-label="Library pagination">
                  <button aria-label="Previous library page" disabled=move || library_page.get()==0 on:click=move |_|library_page.update(|page|*page=page.saturating_sub(1))><ArrowLeft size=13/>"Previous"</button>
                  <span>{move || {let count=if library_personal.get(){saved_imports.get().len()}else{BUILTIN_RUDIMENTS.len()};let pages=count.div_ceil(LIBRARY_PAGE_SIZE).max(1);let page=library_page.get().min(pages-1);format!("Page {} of {}",page+1,pages)}}</span>
                  <button aria-label="Next library page" disabled=move || {let count=if library_personal.get(){saved_imports.get().len()}else{BUILTIN_RUDIMENTS.len()};library_page.get()+1>=count.div_ceil(LIBRARY_PAGE_SIZE).max(1)} on:click=move |_|{let count=if library_personal.get_untracked(){saved_imports.get_untracked().len()}else{BUILTIN_RUDIMENTS.len()};let pages=count.div_ceil(LIBRARY_PAGE_SIZE).max(1);library_page.update(|page|*page=(*page+1).min(pages-1));}>"Next"<ArrowRight size=13/></button>
                </div>
              </section>
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
                </div>
                <h3>"Recent sessions"</h3>
                <ol class="stats-recent">
                  {move || if history.get().is_empty(){
                    view!{<li class="stats-empty">"Complete a performance to see your statistics here."</li>}.into_any()
                  }else{
                    history.get().iter().rev().take(8).map(|session|view!{
                      <li>
                        <span><strong>{session.exercise_title.clone()}</strong><small>{let (perfect,good,early,late)=timing_hit_counts(&session.summary);format!("{:.0} BPM · {} · {} · {} perfect, {} good, {} early, {} late, {} missed",session.bpm,match session.input_mode{InputSource::Demo=>"Demo",InputSource::Midi=>"MIDI"},session.ended_at.get(..10).unwrap_or_default(),perfect,good,early,late,session.summary.missed)}</small></span>
                        <b>{format!("{}/100",display_score(session.summary.score))}</b>
                      </li>
                    }).collect_view().into_any()
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
            <section><h3>"Metronome"</h3>
              <MetronomeSettings bpm=bpm count_in=count_in subdivision=subdivision timbre=timbre metronome=metronome click_volume=click_volume snare_volume=snare_volume snare_muted=snare_muted error=metronome_preview_error change_tempo=change_tempo.clone() save=save_current_settings.clone()/>
            </section>
            <section><h3>"Reference snare"</h3><label class="toggle-row"><span>"Play app snare"<small>"Mute this if your drum module is audible."</small></span><input type="checkbox" prop:checked=move || !snare_muted.get() on:change=move |event|{snare_muted.set(!event_target_checked(&event));set_volumes(if snare_muted.get_untracked(){0.0}else{snare_volume.get_untracked()},click_volume.get_untracked());save_snare_mute();}/></label><label>"Play snare"<select aria-label="Reference snare timing" prop:value=move || snare_reference_mode.get() on:change=move |event|{let value=event_target_value(&event);snare_reference_mode.set(if value=="notes"{"notes".into()}else{"hits".into()});save_snare_reference_mode();}><option value="hits">"With your hits"</option><option value="notes">"On written notes"</option></select><small class="help">{move || if snare_reference_mode.get()=="notes"{"Plays each written snare note as a reference; your strikes are still scored silently."}else{"Plays immediately when Rudiment receives your keyboard or MIDI strike."}}</small></label><label>"Snare volume"<input type="range" min="0" max="1" step="0.01" prop:value=move || snare_volume.get() on:input=move |event|{let value=event_target_value(&event).parse().unwrap_or(0.72);snare_volume.set(value);set_volumes(if snare_muted.get_untracked(){0.0}else{value},click_volume.get_untracked());save_snare_volume();}/></label></section>
            <section><h3>"Practice repeats"</h3>
              <label class="toggle-row"><span>"Repeat until successful"<small>"A failed bar is tried again; every failed attempt still reduces the final performance score."</small></span><input aria-label="Repeat bar until Good" type="checkbox" prop:checked=move || repeat_bar_until_good.get() on:change={let save=save_repeat_bar.clone();move |event|{repeat_bar_until_good.set(event_target_checked(&event));save();}}/></label>
              <label>"Required accuracy"<select aria-label="Repeat tolerance" prop:value=move || match repeat_tolerance.get(){RepeatTolerance::Relaxed=>"relaxed",RepeatTolerance::Standard=>"standard",RepeatTolerance::Strict=>"strict"} on:change=move |event|{repeat_tolerance.set(match event_target_value(&event).as_str(){"relaxed"=>RepeatTolerance::Relaxed,"strict"=>RepeatTolerance::Strict,_=>RepeatTolerance::Standard});save_repeat_tolerance();}><option value="relaxed">"Relaxed · no misses"</option><option value="standard">"Standard · Good or Perfect"</option><option value="strict">"Strict · Perfect only"</option></select><small class="help">"Extra hits and wrong drums always fail the current attempt. This setting does not change the timing windows."</small></label>
            </section>
            <section><h3>"MIDI drums"</h3><p class="help">"Use the MIDI status control in the top bar to connect or rescan. Rudiment never uploads your events."</p><p class="midi-state">{move || midi_status.get()}</p>
              <label class=move || if midi_ports.get().is_empty(){"hidden"}else{""}>"Input device"<select prop:value=move || selected_midi.get().unwrap_or_default() on:change=move |event|select_midi(&event_target_value(&event))>{move || midi_ports.get().into_iter().map(|port|view!{<option value=port.id.clone()>{format!("{} · {}",port.manufacturer,port.name)}</option>}).collect_view()}</select></label>
              <div class="learn-card"><strong>{move || if midi_learn.get(){"Hit your snare now…"}else{"Quick MIDI Learn"}}</strong><span>{move || format!("Current snare: note {}, channel {}",snare_note.get(),snare_channel.get())}</span><button on:click=move |_|midi_learn.set(true)>{move || if midi_learn.get(){"Listening…"}else{"Learn snare"}}</button></div>
              <details><summary>"MIDI diagnostics"</summary><div class="diagnostics">{move || if diagnostics.get().is_empty(){view!{<p>"No MIDI messages received yet."</p>}.into_any()}else{diagnostics.get().into_iter().map(|line|view!{<code>{line}</code>}).collect_view().into_any()}}</div></details>
            </section>
            <section><h3>"Latency calibration"</h3><p class="help">"Tap your snare or J with each of eight clicks. This estimates input offset; it cannot remove all external hardware latency."</p><strong class="cal-value">{move || format!("{:+.0} ms",latency_ms.get())}</strong><div class="button-row"><button on:click=move |_|{calibration_errors.set(vec![]);let promise=start_calibration();spawn_local(async move{if let Ok(value)=JsFuture::from(promise).await{let array=Array::from(&value);calibration_targets.set(array.iter().filter_map(|item|item.as_f64()).collect());}});}>"Start 8-tap calibration"</button><button on:click=move |_|{latency_ms.set(0.0);calibration_targets.set(vec![]);save_cal_reset();}>"Reset"</button></div><label>"Manual offset"<input type="range" min="-200" max="200" step="1" prop:value=move || latency_ms.get() on:input=move |event|{latency_ms.set(event_target_value(&event).parse().unwrap_or(0.0));save_cal_manual();}/></label></section>
            <section><h3>"Timing feedback"</h3><p class="help">{format!("Perfect hits are green (±{DEFAULT_PERFECT_MS:.0} ms by default), Good hits light green, Early hits orange and Late hits purple. Unplayed notes remain neutral. Matching closes outside ±{DEFAULT_MATCH_MS:.0} ms.")}</p><label>{move || format!("Perfect window · ±{:.0} ms",excellent_ms.get())}<input type="range" min="5" max="40" step="1" prop:value=move || excellent_ms.get() on:input=move |event|{let value=event_target_value(&event).parse::<f64>().unwrap_or(DEFAULT_PERFECT_MS).min(on_time_ms.get_untracked());excellent_ms.set(value);save_excellent();}/></label><label>{move || format!("Good window · ±{:.0} ms",on_time_ms.get())}<input type="range" min="20" max="60" step="1" prop:value=move || on_time_ms.get() on:input=move |event|{let value=event_target_value(&event).parse::<f64>().unwrap_or(DEFAULT_GOOD_MS);on_time_ms.set(value);if excellent_ms.get_untracked()>value{excellent_ms.set(value);}save_on_time();}/></label></section>
            <section><h3>"Local data"</h3><p class="help">{move || format!("{} completed performance(s) stored only in this browser.",history.get().len())}</p><div class="button-row"><button on:click=move |_|{let promise=export_history();spawn_local(async move{let _=JsFuture::from(promise).await;});}>"Export JSON"</button><button class="danger" on:click=move |_|{if web_sys::window().and_then(|window|window.confirm_with_message("Clear all Rudiment performance history from this browser?").ok()).unwrap_or(false){let promise=clear_history();spawn_local(async move{if JsFuture::from(promise).await.is_ok(){history.set(vec![]);}});}}>"Clear history"</button></div><button class="wide-button" on:click={move |_|{let promise=reset_local_setup();app_mode.set(AppMode::Practice);bpm.set(60.0);count_in.set(1);subdivision.set(1);timbre.set("woodblock".into());metronome.set(true);click_volume.set(0.72);snare_volume.set(0.72);snare_muted.set(false);snare_reference_mode.set("hits".into());loops.set(1);repeat_bar_until_good.set(false);repeat_tolerance.set(RepeatTolerance::Standard);selection_start_bar.set(0);selection_end_bar.set(usize::MAX);latency_ms.set(0.0);on_time_ms.set(DEFAULT_GOOD_MS);excellent_ms.set(DEFAULT_PERFECT_MS);snare_note.set(38);snare_channel.set(10);selected_midi.set(None);midi_status.set("Not connected".into());spawn_local(async move{let _=JsFuture::from(promise).await;});}}>"Reset settings & mappings"</button></section>
          </div>
        </aside><button class=move || if settings_open.get(){"drawer-backdrop"}else{"drawer-backdrop hidden"} aria-label="Close settings" tabindex="-1" on:click=move |_|{settings_open.set(false);restore_settings_focus();}></button>
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

    fn observed_at(timestamp_ms: f64) -> ObservedHit {
        ObservedHit {
            timestamp_ms,
            instrument: Instrument::Snare,
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
                        range_start: 0.0,
                        range_length: 4.0,
                        latency_ms: 0.0,
                        config: ScoringConfig::default(),
                    },
                )
            })
            .collect()
    }

    #[test]
    fn reference_snare_uses_only_snare_notes_in_the_active_segment() {
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
        expected.push(kick);
        let mut next_bar = expected[0].clone();
        next_bar.id = "next-bar".into();
        next_bar.beat = 4.0;
        expected.push(next_bar);

        assert_eq!(
            reference_snare_notes(&expected, 0.0, 4.0),
            vec![
                ReferenceSnareNote {
                    beat: 0.0,
                    velocity: 42,
                },
                ReferenceSnareNote {
                    beat: 1.0,
                    velocity: 118,
                },
            ]
        );
    }

    #[test]
    fn live_tempo_change_preserves_elapsed_history_and_retimes_future_beats() {
        let exercise = paradiddle();
        let original = scaled_tempo_map(&exercise, 1, 60.0);
        let changed = retime_tempo_map(&original, &exercise, 1, 4.0, 120.0);
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
                range_start: 0.0,
                range_length: 4.0,
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
        let result = live_judgement_for_hit(
            &expected,
            &mut matched,
            &observed_at(8_000.0),
            &LiveTimingContext {
                session_start_ms: 0.0,
                bpm: 240.0,
                tempo_map: &[],
                range_start: 28.0,
                range_length: 4.0,
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
    fn selected_range_repetitions_have_unique_ids_and_contiguous_timing() {
        let exercise = paradiddle();
        let (start_bar, end_bar, start_beat, end_beat) = selected_bar_range(&exercise, 1, 2);
        assert_eq!((start_bar, end_bar), (1, 2));
        assert_eq!((start_beat, end_beat), (4.0, 12.0));
        let hits = expanded_range_hits(&exercise, start_beat, end_beat, 2);
        assert!(hits.iter().all(|hit| (4.0..20.0).contains(&hit.beat)));
        assert_eq!(
            hits.iter().map(|hit| &hit.id).collect::<HashSet<_>>().len(),
            hits.len()
        );
        assert!(hits.iter().any(|hit| hit.beat >= 12.0));
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
    fn progress_uses_the_latest_ten_scores_for_the_same_file_and_tempo() {
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
        history.push(other_tempo);
        assert_eq!(
            progress_scores(&current, &history),
            (2..12).map(|value| value as f64).collect::<Vec<_>>()
        );

        let lone = session_record("lone", "new-file", SCORING_VERSION, 78.0);
        assert_eq!(progress_scores(&lone, &[]), vec![78.0]);
        assert_eq!(progress_chart_points(&[78.0]).len(), 1);
        assert_eq!(progress_chart_points(&[60.0, 80.0]).len(), 2);
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
    fn full_session_statistics_have_one_production_call_site() {
        let production_source = include_str!("main.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("production source");
        assert_eq!(
            production_source
                .matches("score_session_with_tempo_map(")
                .count(),
            1
        );
    }
}
