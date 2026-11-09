use js_sys::{Array, Date, Function, Promise, Uint8Array};
use leptos::prelude::*;
use lucide_leptos::{
    ArrowLeft, ArrowRight, ChartNoAxesCombined, Clock3, FolderOpen, Minus, Pause, Play, Plus,
    RotateCcw, Settings as SettingsIcon, X,
};
use rudiment_core::{
    Exercise, ExpectedHit, InputSource, Instrument, Judgement, ObservedHit, SCORING_VERSION,
    ScoringConfig, SessionSummary, TempoChange, beat_at_elapsed_ms, elapsed_ms_at_beat, paradiddle,
    parse_midi, score_session_with_tempo_map,
};
use serde::{Deserialize, Serialize};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::{JsFuture, spawn_local};

type FrameCallback = Closure<dyn FnMut(f64, f64, f64)>;
type DoneCallback = Closure<dyn FnMut()>;
type CallbackSlot<T> = Rc<RefCell<Option<T>>>;
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
    #[wasm_bindgen(js_namespace = rudiment, js_name = startCalibration)]
    fn start_calibration() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = renderScore)]
    fn render_score(exercise: JsValue);
    #[wasm_bindgen(js_namespace = rudiment, js_name = updateScore)]
    fn update_score(beat: f64, judgements: JsValue);
    #[wasm_bindgen(js_namespace = rudiment, js_name = clearScoreFeedback)]
    fn clear_score_feedback();
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
    bpm: Option<f64>,
    count_in_bars: Option<u8>,
    subdivision: Option<u8>,
    timbre: Option<String>,
    metronome: Option<bool>,
    click_volume: Option<f64>,
    snare_volume: Option<f64>,
    snare_muted: Option<bool>,
    loops: Option<u8>,
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

fn count_with_percentage(count: usize, expected: usize) -> String {
    if expected == 0 {
        return format!("{count} · —");
    }
    format!("{count} · {:.0}%", 100.0 * count as f64 / expected as f64)
}

fn percentage(count: usize, total: usize) -> f64 {
    if total == 0 {
        0.0
    } else {
        100.0 * count as f64 / total as f64
    }
}

fn timing_focus(summary: &SessionSummary) -> String {
    match summary.signed_mean_error_ms {
        Some(value) if value > 5.0 => format!(
            "You're averaging {:.0} ms behind the beat. Try to land each stroke with the click rather than after it.",
            value
        ),
        Some(value) if value < -5.0 => format!(
            "You're averaging {:.0} ms ahead of the beat. Relax into the click and let it arrive with your stroke.",
            value.abs()
        ),
        Some(_) => "Your average timing is centred on the beat. Keep the same relaxed pulse as the tempo rises.".into(),
        None => "Play the next run with the click to unlock a personalised timing focus.".into(),
    }
}

fn display_score(score: Option<f64>) -> String {
    score
        .map(|value| format!("{value:.1}"))
        .unwrap_or_else(|| "—".into())
}

fn exercise_beats(exercise: &Exercise) -> f64 {
    let last = exercise.hits.iter().map(|hit| hit.beat).fold(0.0, f64::max);
    let bar_beats = beats_per_bar(exercise);
    ((last + 0.001) / bar_beats).ceil().max(1.0) * bar_beats
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

fn tempo_label(instrument: Instrument) -> &'static str {
    match instrument {
        Instrument::Snare => "snare",
        Instrument::Kick => "kick",
        Instrument::ClosedHat => "closed hi-hat",
        Instrument::OpenHat => "open hi-hat",
        Instrument::PedalHat => "pedal hi-hat",
        Instrument::Ride => "ride",
        Instrument::Crash => "crash",
        Instrument::Tom => "toms",
        Instrument::OtherPercussion => "other percussion",
        Instrument::Unknown => "unknown notes",
    }
}

fn json_compatible<T: Serialize>(value: &T) -> Result<JsValue, serde_wasm_bindgen::Error> {
    value.serialize(&serde_wasm_bindgen::Serializer::json_compatible())
}

fn expanded_hits(exercise: &Exercise, loops: u8) -> Vec<ExpectedHit> {
    let length = exercise_beats(exercise);
    let mut result = Vec::with_capacity(exercise.hits.len() * loops as usize);
    for repetition in 0..loops {
        for hit in &exercise.hits {
            let mut copy = hit.clone();
            copy.id = format!("{}-loop{}", copy.id, repetition + 1);
            copy.beat += repetition as f64 * length;
            result.push(copy);
        }
    }
    result
}

fn visible_judgements(
    summary: SessionSummary,
    now_ms: f64,
    session_start_ms: f64,
    bpm: f64,
    tempo_map: &[TempoChange],
    exercise_length: f64,
    latency_ms: f64,
) -> Vec<LiveJudgement> {
    summary
        .results
        .into_iter()
        .filter(|result| match result.expected_beat {
            Some(expected_beat) if result.judgement == Judgement::Missed => {
                now_ms
                    >= session_start_ms
                        + elapsed_ms_at_beat(expected_beat, bpm, tempo_map)
                        + result.window_ms
            }
            Some(_) => true,
            None => result.observed_ms.is_some(),
        })
        .map(|result| LiveJudgement {
            expected_id: result.expected_id.map(|id| {
                id.rsplit_once("-loop")
                    .filter(|(_, repetition)| repetition.parse::<u8>().is_ok())
                    .map(|(base, _)| base.to_string())
                    .unwrap_or(id)
            }),
            expected_beat: result.expected_beat.map(|beat| beat % exercise_length),
            actual_beat: result.observed_ms.map(|observed_ms| {
                beat_at_elapsed_ms(observed_ms - latency_ms - session_start_ms, bpm, tempo_map)
                    % exercise_length
            }),
            instrument: result.instrument,
            judgement: result.judgement,
            error_ms: result.error_ms.unwrap_or(0.0),
            excellent: result.excellent,
            note_score: result.note_score,
            event_id: result.observed_ms.map(|value| format!("{value:.3}")),
        })
        .collect()
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
    let loops = RwSignal::new(1u8);
    let progress = RwSignal::new(-1.0);
    let run_start_ms = RwSignal::new(0.0);
    let observed = RwSignal::new(Vec::<ObservedHit>::new());
    let live = RwSignal::new(Vec::<LiveJudgement>::new());
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
    let welcome = RwSignal::new(true);
    let import_error = RwSignal::new(None::<String>);
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
    let on_time_ms = RwSignal::new(40.0);
    let excellent_ms = RwSignal::new(20.0);
    let runtime_tempo_map = RwSignal::new(scaled_tempo_map(&paradiddle(), 1, 60.0));

    let save_current_settings = Rc::new(move || {
        persist_settings(Settings {
            bpm: Some(bpm.get_untracked()),
            count_in_bars: Some(count_in.get_untracked()),
            subdivision: Some(subdivision.get_untracked()),
            timbre: Some(timbre.get_untracked()),
            metronome: Some(metronome.get_untracked()),
            click_volume: Some(click_volume.get_untracked()),
            snare_volume: Some(snare_volume.get_untracked()),
            snare_muted: Some(snare_muted.get_untracked()),
            loops: Some(loops.get_untracked()),
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
        let expected = expanded_hits(&exercise.get_untracked(), loops.get_untracked());
        let tempo_map = runtime_tempo_map.get_untracked();
        let scoring_bpm = tempo_map
            .first()
            .map(|change| change.bpm)
            .unwrap_or(bpm.get_untracked());
        let summary = score_session_with_tempo_map(
            &expected,
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
        };
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
        result_modal_open.set(true);
        focus_result_close();
    });

    let frame_holder: CallbackSlot<FrameCallback> = Rc::new(RefCell::new(None));
    let done_holder: CallbackSlot<DoneCallback> = Rc::new(RefCell::new(None));
    let launch_run: Rc<dyn Fn(f64, u8)> = {
        let finish_run = finish_run.clone();
        let frame_holder = frame_holder.clone();
        let done_holder = done_holder.clone();
        Rc::new(move |offset, count_bars| {
            let last_judgement_ms = Rc::new(Cell::new(f64::NEG_INFINITY));
            phase.set(if count_bars > 0 {
                "count_in".into()
            } else {
                "running".into()
            });
            let current_exercise = exercise.get_untracked();
            let total = exercise_beats(&current_exercise) * loops.get_untracked() as f64;
            let tempo_map = expanded_tempo_map(&current_exercise, loops.get_untracked());
            if offset <= 0.000_001 {
                runtime_tempo_map.set(scaled_tempo_map(
                    &current_exercise,
                    loops.get_untracked(),
                    bpm.get_untracked(),
                ));
            }
            let tempo_events = tempo_map
                .iter()
                .map(|change| serde_json::json!({"beat":change.beat,"bpm":change.bpm}))
                .collect::<Vec<_>>();
            let options=json_compatible(&serde_json::json!({"bpm":bpm.get_untracked(),"sourceBpm":current_exercise.tempo_bpm,"tempoMap":tempo_events,"countInBars":count_bars,"meter":beats_per_bar(&current_exercise),"totalBeats":total,"offsetBeat":offset,"subdivision":subdivision.get_untracked(),"metronome":metronome.get_untracked(),"timbre":timbre.get_untracked()})).unwrap_or(JsValue::NULL);
            let frame = Closure::wrap(Box::new(move |beat: f64, now: f64, start: f64| {
                progress.set(beat);
                let score_tempo_map = runtime_tempo_map.get_untracked();
                let scoring_bpm = score_tempo_map
                    .first()
                    .map(|change| change.bpm)
                    .unwrap_or(bpm.get_untracked());
                let session_start =
                    start - elapsed_ms_at_beat(offset, scoring_bpm, &score_tempo_map);
                run_start_ms.set(session_start);
                if beat >= 0.0 && phase.get_untracked() == "count_in" {
                    phase.set("running".into());
                }
                let base = exercise_beats(&exercise.get_untracked());
                let local = if beat >= 0.0 { beat % base } else { beat };
                let mut judgement_values = JsValue::UNDEFINED;
                if beat >= 0.0 && now - last_judgement_ms.get() >= 30.0 {
                    last_judgement_ms.set(now);
                    let expected = expanded_hits(&exercise.get_untracked(), loops.get_untracked());
                    let summary = score_session_with_tempo_map(
                        &expected,
                        &observed.get_untracked(),
                        scoring_bpm,
                        session_start,
                        latency_ms.get_untracked(),
                        ScoringConfig {
                            perfect_ms: excellent_ms.get_untracked(),
                            good_ms: on_time_ms.get_untracked(),
                            ..ScoringConfig::default()
                        },
                        &score_tempo_map,
                    );

                    let visible = visible_judgements(
                        summary,
                        now,
                        session_start,
                        scoring_bpm,
                        &score_tempo_map,
                        base,
                        latency_ms.get_untracked(),
                    );
                    live.set(visible.clone());
                    judgement_values =
                        serde_wasm_bindgen::to_value(&visible).unwrap_or(JsValue::NULL);
                }
                update_score(local, judgement_values);
            }) as Box<dyn FnMut(f64, f64, f64)>);
            let done_finish = finish_run.clone();
            let done = Closure::wrap(Box::new(move || done_finish()) as Box<dyn FnMut()>);
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

    let reset_run: Rc<dyn Fn()> = Rc::new(move || {
        stop_audio();
        result_modal_open.set(false);
        observed.set(vec![]);
        live.set(vec![]);
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
            live.set(vec![]);
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
            observed.set(vec![]);
            live.set(vec![]);
            result.set(None);
            progress.set(-1.0);
            clear_score_feedback();
            launch(0.0, count_bars);
        })
    };

    let save_for_hit = save_current_settings.clone();
    let start_for_hit = start_new_run.clone();
    let hit_input: HitInput = Rc::new(
        move |timestamp, instrument, velocity, source, raw_note, raw_channel| {
            if !snare_muted.get_untracked() {
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
            if phase.get_untracked() == "complete" && result_modal_open.get_untracked() {
                result_modal_open.set(false);
                restore_result_focus();
                return;
            }
            if matches!(phase.get_untracked().as_str(), "idle" | "complete") {
                input_mode.set(source);
                start_for_hit(count_in.get_untracked());
                return;
            }
            if phase.get_untracked() != "running" {
                return;
            }
            let event = ObservedHit {
                timestamp_ms: timestamp,
                instrument,
                velocity,
                source,
                raw_note,
                raw_channel,
            };
            observed.update(|items| items.push(event));
            let expected = expanded_hits(&exercise.get_untracked(), loops.get_untracked());
            let session_start = run_start_ms.get_untracked();
            let tempo_map = runtime_tempo_map.get_untracked();
            let scoring_bpm = tempo_map
                .first()
                .map(|change| change.bpm)
                .unwrap_or(bpm.get_untracked());
            let summary = score_session_with_tempo_map(
                &expected,
                &observed.get_untracked(),
                scoring_bpm,
                session_start,
                latency_ms.get_untracked(),
                ScoringConfig {
                    perfect_ms: excellent_ms.get_untracked(),
                    good_ms: on_time_ms.get_untracked(),
                    ..ScoringConfig::default()
                },
                &tempo_map,
            );
            let base = exercise_beats(&exercise.get_untracked());
            let visible = visible_judgements(
                summary,
                timestamp,
                session_start,
                scoring_bpm,
                &tempo_map,
                base,
                latency_ms.get_untracked(),
            );
            live.set(visible);
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
                live.set(vec![]);
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
                } else if event.disconnected_id.is_some() {
                    midi_status.set("Drums disconnected — reconnecting when available".into());
                    if matches!(phase.get_untracked().as_str(), "running" | "count_in") {
                        stop_audio();
                        phase.set("paused".into());
                    }
                } else if let Some(id) = event.selected_id {
                    let port = event.ports.iter().find(|port| port.id == id);
                    midi_status.set(
                        port.map(|port| format!("{} · connected", port.name))
                            .unwrap_or_else(|| "Connected".into()),
                    );
                } else if event.permission.as_deref() == Some("prompt") {
                    midi_status.set("MIDI ready · connect to grant access".into());
                } else if event.permission.as_deref() == Some("unsupported") {
                    midi_status.set("Web MIDI unavailable in this browser".into());
                } else {
                    midi_status.set("No MIDI drums detected".into());
                }
                return;
            }
            let line = format!(
                "{:.1} ms · {} ch{} · note {} · velocity {} · raw {:?}",
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
                    if let Some(v) = saved.loops {
                        loops.set(v);
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

    let key_launch = launch_run.clone();
    let key_reset_run = reset_run.clone();
    let key_start_new_run = start_new_run.clone();
    let key_hit = hit_input.clone();
    let keyboard = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
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
                    "running" | "count_in" => {
                        stop_audio();
                        phase.set("paused".into());
                    }
                    "paused" => key_launch(progress.get_untracked().max(0.0), 0),
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

    let visibility = Closure::wrap(Box::new(move || {
        if web_sys::window()
            .and_then(|window| window.document())
            .map(|document| document.visibility_state() == web_sys::VisibilityState::Hidden)
            .unwrap_or(false)
            && matches!(phase.get_untracked().as_str(), "running" | "count_in")
        {
            stop_audio();
            phase.set("paused".into());
        }
    }) as Box<dyn FnMut()>);
    if let Some(document) = web_sys::window().and_then(|window| window.document()) {
        let _ = document.add_event_listener_with_callback(
            "visibilitychange",
            visibility.as_ref().unchecked_ref(),
        );
    }
    visibility.forget();

    let start_click = launch_run.clone();
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
    let save_tempo_down = save_current_settings.clone();
    let save_tempo_input = save_current_settings.clone();
    let save_tempo_commit = save_current_settings.clone();
    let save_tempo_up = save_current_settings.clone();
    let tempo_down = change_tempo.clone();
    let tempo_input = change_tempo.clone();
    let tempo_commit = change_tempo.clone();
    let tempo_up = change_tempo.clone();
    let stop_for_connect = stop_for_navigation.clone();
    let stop_for_stats = stop_for_navigation.clone();
    let stop_for_settings = stop_for_navigation.clone();
    let stop_for_import = stop_for_navigation.clone();
    let stop_for_library = stop_for_navigation.clone();
    let stop_for_track = stop_for_navigation.clone();
    let save_welcome = save_current_settings.clone();
    let save_metronome = save_current_settings.clone();
    let save_countin = save_current_settings.clone();
    let save_subdivision = save_current_settings.clone();
    let save_timbre = save_current_settings.clone();
    let save_click_volume = save_current_settings.clone();
    let save_snare_mute = save_current_settings.clone();
    let save_snare_volume = save_current_settings.clone();
    let save_loops = save_current_settings.clone();
    let save_cal_reset = save_current_settings.clone();
    let save_cal_manual = save_current_settings.clone();
    let save_on_time = save_current_settings.clone();
    let save_excellent = save_current_settings.clone();
    view! {
      <main class="app">
        <header class="topbar">
          <a class="brand" href="#" aria-label="Rudiment home"><span class="brand-mark">"R"</span><span>"Rudiment"</span></a>
          <div class="exercise-heading"><span class="eyebrow">"CURRENT EXERCISE"</span><strong>{move || exercise.get().title}</strong><small>"4/4 · sticking and accents"</small></div>
          <div class="tempo-control"><button aria-label="Decrease tempo" on:click=move |_|{tempo_down(bpm.get_untracked()-1.0);save_tempo_down();}><Minus size=17/></button><label><input aria-label="Tempo in BPM" title="Type a tempo from 30 to 240 BPM" type="number" inputmode="numeric" min="30" max="240" step="1" prop:value=move || bpm.get().round().to_string() on:focus=move |event|{let _=event_target::<web_sys::HtmlInputElement>(&event).select();} on:input=move |event|{if let Ok(value)=event_target_value(&event).parse::<f64>(){if (30.0..=240.0).contains(&value){tempo_input(value.round());save_tempo_input();}}} on:change=move |event|{let input=event_target::<web_sys::HtmlInputElement>(&event);let value=input.value().parse::<f64>().unwrap_or_else(|_|bpm.get_untracked()).round().clamp(30.0,240.0);tempo_commit(value);input.set_value(&value.to_string());save_tempo_commit();}/><span>"BPM"</span></label><button aria-label="Increase tempo" on:click=move |_|{tempo_up(bpm.get_untracked()+1.0);save_tempo_up();}><Plus size=17/></button></div>
          <button class=move || if selected_midi.get().is_some(){"status-pill connected"}else{"status-pill"} aria-label="MIDI connection" on:click={let connect=connect_action.clone();move |_|{stop_for_connect();connect();}}><span class="status-dot"></span>{move || midi_status.get()}</button>
          <button id="midi-library-trigger" class="stats-trigger library-topbar-trigger" aria-label="MIDI library" title=move || format!("MIDI library · {} exercises",saved_imports.get().len()+BUILTIN_RUDIMENTS.len()) aria-haspopup="dialog" aria-expanded=move || library_open.get().to_string() on:click=move |_|{
            stop_for_library();
            library_personal.set(false);
            library_page.set(0);
            library_open.set(true);
            focus_library_close();
          }><FolderOpen size=19/><span class="library-count" aria-hidden="true">{move || saved_imports.get().len()+BUILTIN_RUDIMENTS.len()}</span></button>
          <button id="stats-trigger" class="stats-trigger" aria-label="Stats" title="Practice statistics" aria-haspopup="dialog" aria-expanded=move || stats_open.get().to_string() on:click=move |_|{stop_for_stats();stats_open.set(true);focus_stats_close();}><ChartNoAxesCombined size=19/></button>
          <button class="stats-trigger settings-trigger" aria-label="Open settings" title="Practice settings" on:click=move |_|{stop_for_settings();settings_open.set(true);}><SettingsIcon size=19/></button>
        </header>

          <section class="workspace">
              <aside class=move || if welcome.get(){"welcome"}else{"welcome hidden"}><div><strong>              "A paradiddle is ready."</strong><span>"Space pauses; R restarts."</span></div><button aria-label="Dismiss welcome hint" on:click=move |_|{welcome.set(false);save_welcome();}>"Got it"</button></aside>
            <section id="score-card" class="score-card">
              <div class="score-meta">
                <div>{move || if selected_midi.get().is_some(){view!{<span class="mode-badge">"MIDI INPUT"</span>}.into_any()}else{().into_any()}}<span>{move || if selected_midi.get().is_some(){"Hit your drum to start; incoming kit events use your saved mapping."}else{""}}</span></div>
                <div id="visual-metronome" class="visual-metronome" role="img" aria-label="Visual metronome, stopped">
                  <span class="visual-metronome-sweep" aria-hidden="true"><i></i></span>
                  <span class="visual-metronome-beats" aria-hidden="true">{move || (0..beats_per_bar(&exercise.get()).round().max(1.0) as usize).map(|_|view!{<i></i>}).collect_view()}</span>
                  <small class="visual-metronome-label">"READY"</small>
                </div>
                <div class="score-actions">
                  <details class=move || if imported_bytes.get().is_empty()||(exercise.get().warnings.is_empty()&&exercise.get().playable_tracks.len()<2){"file-details hidden"}else{"file-details"}>
                    <summary>"File details"</summary>
                    <div class="file-details-menu">
                      <label class={move || if exercise.get().playable_tracks.len()>1{"track-picker"}else{"track-picker hidden"}}>
                        <span>"Drum track"</span>
                        <select aria-label="Drum track" prop:value=move || exercise.get().selected_track.to_string() on:change=move |event|{
                          stop_for_track();
                          let index=event_target_value(&event).parse::<usize>().unwrap_or(0);
                          if let Ok(parsed)=parse_midi(&imported_bytes.get_untracked(),&imported_name.get_untracked(),Some(index)){exercise.set(parsed);}
                        }>
                          {move || exercise.get().playable_tracks.into_iter().map(|track|{
                            let instruments=track.instruments.into_iter().map(tempo_label).collect::<Vec<_>>().join(", ");
                            let label=if instruments.is_empty(){track.name}else{format!("{} — {}",track.name,instruments)};
                            view!{<option value=track.index.to_string()>{label}</option>}
                          }).collect_view()}
                        </select>
                      </label>
                      <ul class=move || if exercise.get().warnings.is_empty(){"file-warnings hidden"}else{"file-warnings"}>
                        {move || exercise.get().warnings.into_iter().map(|warning|view!{<li>{warning}</li>}).collect_view()}
                      </ul>
                    </div>
                  </details>
                </div>
              </div>
              <div id="notation" class="notation" aria-live="off"></div>
              <div class=move || if import_error.get().is_some(){"error-row"}else{"error-row hidden"}>{move || import_error.get().unwrap_or_default()}</div>
              <div class="position-row"><span>{move || {let bar_beats=beats_per_bar(&exercise.get());let current=(progress.get().max(0.0)/bar_beats).floor() as usize+1;format!("BAR {current} / {}",(exercise_beats(&exercise.get())*loops.get() as f64/bar_beats) as usize)}}</span><div class="timeline"><i style=move || format!("width:{:.2}%",(progress.get().max(0.0)/(exercise_beats(&exercise.get())*loops.get() as f64)*100.0).clamp(0.0,100.0))></i></div><span>{move || format!("{} / {} LOOP",((progress.get().max(0.0)/exercise_beats(&exercise.get())).floor() as u8+1).min(loops.get()),loops.get())}</span></div>
              <div class=move || if result_modal_open.get(){"result-backdrop"}else{"result-backdrop hidden"} on:click=move |_|{result_modal_open.set(false);restore_result_focus();}>
              <section id="result-dialog" class="stats-modal results-modal" role="dialog" aria-modal="true" aria-labelledby="result-title" on:click=|event|event.stop_propagation() on:keydown=move |event:web_sys::KeyboardEvent|{
                  match event.key().as_str(){
                    "Escape"=>{event.prevent_default();result_modal_open.set(false);restore_result_focus();},
                    "Tab"=>trap_dialog_tab(&event,"#result-dialog"),
                    _=>{},
                  }
                }>
                <div class="stats-modal-header results-header"><div><span class="eyebrow">"SESSION SUMMARY"</span><h2 id="result-title">"Practice complete."</h2></div><button id="result-modal-close" aria-label="Close session results" on:click=move |_|{result_modal_open.set(false);restore_result_focus();}><X size=20/></button></div>
                <div class="results-overview">
                  <div class="score-ring" style=move || format!("--score:{}",result.get().and_then(|session|session.summary.score).unwrap_or(0.0))><div><strong>{move || display_score(result.get().and_then(|session|session.summary.score))}</strong><span>"/ 100"</span></div></div>
                  <div class="result-exercise"><h3>{move || result.get().map(|session|session.exercise_title).unwrap_or_default()}</h3><p>{move || result.get().map(|session|format!("{:.0} BPM · {} input",session.bpm,match session.input_mode{InputSource::Demo=>"Demo",InputSource::Midi=>"MIDI"})).unwrap_or_default()}</p><span class="hit-chip">{move || result.get().map(|session|format!("{} / {} notes hit ({:.0}%)",session.summary.matched,session.summary.expected,percentage(session.summary.matched,session.summary.expected))).unwrap_or_else(||"—".into())}</span></div>
                  <div class="result-comparison"><div><span>"PREVIOUS RUN"</span><strong>{move || comparison.get().map(|delta|if delta > 0.05{format!("+{delta:.1} pts") }else if delta < -0.05{format!("{delta:.1} pts")}else{"No change".into()}).unwrap_or_else(||"First run".into())}</strong></div><div><span>"PERSONAL BEST"</span><strong>{move || result.get().map(|session|{let best=history.get().iter().filter(|item|item.exercise_hash==session.exercise_hash&&(item.bpm-session.bpm).abs()<0.01&&item.input_mode==session.input_mode&&item.scoring_version==session.scoring_version&&(item.tolerance_ms-session.tolerance_ms).abs()<0.01).filter_map(|item|item.summary.score).max_by(|left,right|left.total_cmp(right)).or(session.summary.score);display_score(best)}).unwrap_or_else(||"—".into())}</strong></div></div>
                </div>
                <section class="accuracy-section">
                  <div class="result-section-heading"><h3>"Note accuracy"</h3><span>{move || result.get().map(|session|format!("{} expected notes",session.summary.expected)).unwrap_or_default()}</span></div>
                  <div class="accuracy-bar" aria-label="Note accuracy distribution">
                    <i class="perfect" style=move || result.get().map(|session|format!("width:{:.3}%",percentage(timing_hit_counts(&session.summary).0,session.summary.expected))).unwrap_or_default()></i><i class="good" style=move || result.get().map(|session|format!("width:{:.3}%",percentage(timing_hit_counts(&session.summary).1,session.summary.expected))).unwrap_or_default()></i><i class="early" style=move || result.get().map(|session|format!("width:{:.3}%",percentage(timing_hit_counts(&session.summary).2,session.summary.expected))).unwrap_or_default()></i><i class="late" style=move || result.get().map(|session|format!("width:{:.3}%",percentage(timing_hit_counts(&session.summary).3,session.summary.expected))).unwrap_or_default()></i><i class="missed" style=move || result.get().map(|session|format!("width:{:.3}%",percentage(session.summary.missed,session.summary.expected))).unwrap_or_default()></i>
                  </div>
                  <div class="accuracy-legend">
                    <article class="perfect"><span><i></i>"Perfect"</span><strong>{move || result.get().map(|session|count_with_percentage(timing_hit_counts(&session.summary).0,session.summary.expected)).unwrap_or_else(||"—".into())}</strong></article><article class="good"><span><i></i>"Good"</span><strong>{move || result.get().map(|session|count_with_percentage(timing_hit_counts(&session.summary).1,session.summary.expected)).unwrap_or_else(||"—".into())}</strong></article><article class="early"><span><i></i>"Early"</span><strong>{move || result.get().map(|session|count_with_percentage(timing_hit_counts(&session.summary).2,session.summary.expected)).unwrap_or_else(||"—".into())}</strong></article><article class="late"><span><i></i>"Late"</span><strong>{move || result.get().map(|session|count_with_percentage(timing_hit_counts(&session.summary).3,session.summary.expected)).unwrap_or_else(||"—".into())}</strong></article><article class="missed"><span><i></i>"Missed"</span><strong>{move || result.get().map(|session|count_with_percentage(session.summary.missed,session.summary.expected)).unwrap_or_else(||"—".into())}</strong></article>
                  </div>
                </section>
                <section class="timing-section">
                  <div class="result-section-heading"><h3>"Timing accuracy"</h3><span>"Matched notes only"</span></div>
                  <div class="timing-cards"><article><span>"AVERAGE OFFSET"</span><strong>{move || result.get().and_then(|session|session.summary.signed_mean_error_ms).map(|value|if value < -0.5{format!("{:.0} ms",value.abs())}else if value > 0.5{format!("{value:.0} ms")}else{"On time".into()}).unwrap_or_else(||"—".into())}</strong><small>{move || result.get().and_then(|session|session.summary.signed_mean_error_ms).map(|value|if value < -0.5{"early"}else if value > 0.5{"late"}else{"centred"}).unwrap_or("")}</small></article><article><span>"MEAN ABSOLUTE ERROR"</span><strong>{move || result.get().and_then(|session|session.summary.mean_abs_error_ms).map(|value|format!("{value:.0} ms")).unwrap_or_else(||"—".into())}</strong></article><article><span>"TIMING VARIABILITY"</span><strong>{move || result.get().and_then(|session|session.summary.timing_consistency_ms).map(|value|format!("{value:.0} ms")).unwrap_or_else(||"—".into())}</strong><small>"SD"</small></article></div>
                  <p class="timing-focus"><span aria-hidden="true"><Clock3 size=15/></span><strong>"Focus for next time:"</strong>{move || result.get().map(|session|timing_focus(&session.summary)).unwrap_or_default()}</p>
                </section>
                <footer class="results-footer"><p>{move || result.get().map(|session|format!("{} extra hit{}    {} wrong drum{}",session.summary.extras,if session.summary.extras==1{""}else{"s"},session.summary.wrong_instrument,if session.summary.wrong_instrument==1{""}else{"s"})).unwrap_or_default()}</p><div class="result-actions"><button on:click=move |_|{result_modal_open.set(false);restore_result_focus();}>"Back to score"</button><button class="primary" on:click={let start=start_new_run.clone();move |_|{observed.set(vec![]);live.set(vec![]);result.set(None);progress.set(-1.0);start(count_in.get_untracked());}}>"Practise again"</button></div></footer>
                </section>
              </div>
            </section>
            <section class="practice-controls">
              <div class="transport">
                <button class="restart-button" on:click={let reset=reset_run.clone();move |_|reset()}><span class="transport-action-label"><span class="transport-icon" aria-hidden="true"><RotateCcw size=24/></span><span>"Restart"</span></span><small>"Clear current run"</small></button>
                <button class="primary start-button" on:click={let launch=start_click.clone();let start=start_new_run.clone();move |_|match phase.get_untracked().as_str(){"running"|"count_in"=>{stop_audio();phase.set("paused".into());},"paused"=>launch(progress.get_untracked().max(0.0),0),_=>{observed.set(vec![]);live.set(vec![]);result.set(None);input_mode.set(if selected_midi.get_untracked().is_some(){InputSource::Midi}else{InputSource::Demo});start(count_in.get_untracked());}}}><span class="transport-action-label"><span class="transport-icon" aria-hidden="true">{move || if matches!(phase.get().as_str(),"running"|"count_in"){view!{<Pause size=21/>}.into_any()}else{view!{<Play size=21 fill="currentColor"/>}.into_any()}}</span><span>{move || match phase.get().as_str(){"running"|"count_in"=>"Pause","paused"=>"Resume",_=>"Start practice"}}</span></span><small class=move || if phase.get()=="idle"{"start-instruction"}else{"start-instruction hidden"}>"Hit your drum or press J to start"</small></button>
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
                      let display_name=format!("{}.mid",item.title);
                      view!{
                        <article class="library-card bundled-entry" role="listitem">
                          <button class="library-load" on:click=move |_|{
                            library_open.set(false);
                            restore_library_focus();
                            load_bundled_from_library(&midi_url,&display_name);
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
                  <div><span class="eyebrow">"YOUR PRACTICE"</span><h2 id="stats-title">"Practice statistics"</h2></div>
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
                    view!{<li class="stats-empty">"Complete a practice session to see your statistics here."</li>}.into_any()
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

        <aside class=move || if settings_open.get(){"settings-drawer open"}else{"settings-drawer"} aria-hidden=move || (!settings_open.get()).to_string()>
          <div class="drawer-head"><div><span class="eyebrow">"PRACTICE SETUP"</span><h2>"Sound & drums"</h2></div><button aria-label="Close settings" on:click=move |_|settings_open.set(false)><X size=22/></button></div>
          <div class="drawer-scroll">
            <section><h3>"Metronome"</h3><label class="toggle-row"><span>"Click during exercise"</span><input type="checkbox" prop:checked=move || metronome.get() on:change=move |event|{metronome.set(event_target_checked(&event));save_metronome();}/></label>
              <label>"Count-in"<select prop:value=move || count_in.get().to_string() on:change=move |event|{count_in.set(event_target_value(&event).parse().unwrap_or(1));save_countin();}><option value="0">"Off"</option><option value="1">"1 bar"</option><option value="2">"2 bars"</option></select></label>
              <label>"Subdivision"<select prop:value=move || subdivision.get().to_string() on:change=move |event|{subdivision.set(event_target_value(&event).parse().unwrap_or(1));save_subdivision();}><option value="1">"Quarter notes"</option><option value="2">"Eighth notes"</option><option value="4">"Sixteenth notes"</option></select></label>
              <label>"Click timbre"<div class="inline-select"><select prop:value=move || timbre.get() on:change=move |event|{timbre.set(event_target_value(&event));save_timbre();}><option value="woodblock">"Warm woodblock"</option><option value="crisp">"Crisp click"</option><option value="cowbell">"Cowbell"</option></select><button on:click=move |_|{let promise=preview_click(&timbre.get_untracked());spawn_local(async move{let _=JsFuture::from(promise).await;});}>"Preview"</button></div></label>
              <label>"Metronome volume"<input type="range" min="0" max="1" step="0.01" prop:value=move || click_volume.get() on:input=move |event|{let value=event_target_value(&event).parse().unwrap_or(0.72);click_volume.set(value);set_volumes(if snare_muted.get_untracked(){0.0}else{snare_volume.get_untracked()},value);save_click_volume();}/></label>
            </section>
            <section><h3>"Reference snare"</h3><label class="toggle-row"><span>"Play app snare"<small>"Mute this if your drum module is audible."</small></span><input type="checkbox" prop:checked=move || !snare_muted.get() on:change=move |event|{snare_muted.set(!event_target_checked(&event));set_volumes(if snare_muted.get_untracked(){0.0}else{snare_volume.get_untracked()},click_volume.get_untracked());save_snare_mute();}/></label><label>"Snare volume"<input type="range" min="0" max="1" step="0.01" prop:value=move || snare_volume.get() on:input=move |event|{let value=event_target_value(&event).parse().unwrap_or(0.72);snare_volume.set(value);set_volumes(if snare_muted.get_untracked(){0.0}else{value},click_volume.get_untracked());save_snare_volume();}/></label></section>
            <section><h3>"Exercise"</h3><label>"Loops"<select prop:value=move || loops.get().to_string() on:change=move |event|{loops.set(event_target_value(&event).parse().unwrap_or(1));save_loops();}><option value="1">"1 run"</option><option value="2">"2 loops"</option><option value="4">"4 loops"</option></select></label><p class="help">"The score follows playback vertically, keeping the current and next bar in view."</p></section>
            <section><h3>"MIDI drums"</h3><p class="help">"Use the MIDI status control in the top bar to connect or rescan. Rudiment never uploads your events."</p><p class="midi-state">{move || midi_status.get()}</p>
              <label class=move || if midi_ports.get().is_empty(){"hidden"}else{""}>"Input device"<select prop:value=move || selected_midi.get().unwrap_or_default() on:change=move |event|select_midi(&event_target_value(&event))>{move || midi_ports.get().into_iter().map(|port|view!{<option value=port.id.clone()>{format!("{} · {}",port.manufacturer,port.name)}</option>}).collect_view()}</select></label>
              <div class="learn-card"><strong>{move || if midi_learn.get(){"Hit your snare now…"}else{"Quick MIDI Learn"}}</strong><span>{move || format!("Current snare: note {}, channel {}",snare_note.get(),snare_channel.get())}</span><button on:click=move |_|midi_learn.set(true)>{move || if midi_learn.get(){"Listening…"}else{"Learn snare"}}</button></div>
              <details><summary>"MIDI diagnostics"</summary><div class="diagnostics">{move || if diagnostics.get().is_empty(){view!{<p>"No MIDI messages received yet."</p>}.into_any()}else{diagnostics.get().into_iter().map(|line|view!{<code>{line}</code>}).collect_view().into_any()}}</div></details>
            </section>
            <section><h3>"Latency calibration"</h3><p class="help">"Tap your snare or J with each of eight clicks. This estimates input offset; it cannot remove all external hardware latency."</p><strong class="cal-value">{move || format!("{:+.0} ms",latency_ms.get())}</strong><div class="button-row"><button on:click=move |_|{calibration_errors.set(vec![]);let promise=start_calibration();spawn_local(async move{if let Ok(value)=JsFuture::from(promise).await{let array=Array::from(&value);calibration_targets.set(array.iter().filter_map(|item|item.as_f64()).collect());}});}>"Start 8-tap calibration"</button><button on:click=move |_|{latency_ms.set(0.0);calibration_targets.set(vec![]);save_cal_reset();}>"Reset"</button></div><label>"Manual offset"<input type="range" min="-200" max="200" step="1" prop:value=move || latency_ms.get() on:input=move |event|{latency_ms.set(event_target_value(&event).parse().unwrap_or(0.0));save_cal_manual();}/></label></section>
            <section><h3>"Timing feedback"</h3><p class="help">"Perfect hits are green (±20 ms by default), Good hits light green, Early hits orange and Late hits purple. Unplayed notes remain neutral. Matching closes outside ±100 ms."</p><label>{move || format!("Perfect window · ±{:.0} ms",excellent_ms.get())}<input type="range" min="5" max="40" step="1" prop:value=move || excellent_ms.get() on:input=move |event|{let value=event_target_value(&event).parse::<f64>().unwrap_or(20.0).min(on_time_ms.get_untracked());excellent_ms.set(value);save_excellent();}/></label><label>{move || format!("Good window · ±{:.0} ms",on_time_ms.get())}<input type="range" min="20" max="60" step="1" prop:value=move || on_time_ms.get() on:input=move |event|{let value=event_target_value(&event).parse::<f64>().unwrap_or(40.0);on_time_ms.set(value);if excellent_ms.get_untracked()>value{excellent_ms.set(value);}save_on_time();}/></label></section>
            <section><h3>"Local data"</h3><p class="help">{move || format!("{} completed session(s) stored only in this browser.",history.get().len())}</p><div class="button-row"><button on:click=move |_|{let promise=export_history();spawn_local(async move{let _=JsFuture::from(promise).await;});}>"Export JSON"</button><button class="danger" on:click=move |_|{if web_sys::window().and_then(|window|window.confirm_with_message("Clear all Rudiment practice history from this browser?").ok()).unwrap_or(false){let promise=clear_history();spawn_local(async move{if JsFuture::from(promise).await.is_ok(){history.set(vec![]);}});}}>"Clear history"</button></div><button class="wide-button" on:click={move |_|{let promise=reset_local_setup();bpm.set(60.0);count_in.set(1);subdivision.set(1);timbre.set("woodblock".into());metronome.set(true);click_volume.set(0.72);snare_volume.set(0.72);snare_muted.set(false);loops.set(1);latency_ms.set(0.0);on_time_ms.set(40.0);excellent_ms.set(20.0);snare_note.set(38);snare_channel.set(10);selected_midi.set(None);midi_status.set("Not connected".into());spawn_local(async move{let _=JsFuture::from(promise).await;});}}>"Reset settings & mappings"</button></section>
          </div>
        </aside><button class=move || if settings_open.get(){"drawer-backdrop"}else{"drawer-backdrop hidden"} aria-label="Close settings" on:click=move |_|settings_open.set(false)></button>
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
}
