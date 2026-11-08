use js_sys::{Array, Date, Function, Promise, Uint8Array};
use leptos::prelude::*;
use rudiment_core::{
    Exercise, ExpectedHit, InputSource, Instrument, Judgement, ObservedHit, SCORING_VERSION,
    ScoringConfig, SessionSummary, paradiddle, parse_midi, score_session_with_config,
};
use serde::{Deserialize, Serialize};
use std::{cell::{Cell, RefCell}, rc::Rc};
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::{JsFuture, spawn_local};

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = rudiment, js_name = ensureAudio)]
    fn ensure_audio() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = previewSnare)]
    fn preview_snare() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = previewClick)]
    fn preview_click(timbre: &str) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = playSnare)]
    fn play_snare(velocity: u8) -> bool;
    #[wasm_bindgen(js_namespace = rudiment, js_name = setVolumes)]
    fn set_volumes(snare: f64, click: f64);
    #[wasm_bindgen(js_namespace = rudiment, js_name = startAudio)]
    fn start_audio(options: JsValue, frame: &Function, done: &Function) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = stopAudio)]
    fn stop_audio();
    #[wasm_bindgen(js_namespace = rudiment, js_name = startCalibration)]
    fn start_calibration() -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = renderScore)]
    fn render_score(exercise: JsValue, follow: &str);
    #[wasm_bindgen(js_namespace = rudiment, js_name = updateScore)]
    fn update_score(beat: f64, judgements: JsValue);
    #[wasm_bindgen(js_namespace = rudiment, js_name = connectMidi)]
    fn connect_midi(callback: &Function) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = autoConnectMidi)]
    fn auto_connect_midi(callback: &Function) -> Promise;
    #[wasm_bindgen(js_namespace = rudiment, js_name = selectMidi)]
    fn select_midi(id: &str);
    #[wasm_bindgen(js_namespace = rudiment, js_name = chooseMidi)]
    fn choose_midi(callback: &Function);
    #[wasm_bindgen(js_namespace = rudiment, js_name = loadBundled)]
    fn load_bundled(url: &str, callback: &Function);
    #[wasm_bindgen(js_namespace = rudiment, js_name = bindDrop)]
    fn bind_drop(callback: &Function);
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
    follow: Option<String>,
    latency_ms: Option<f64>,
    on_time_ms: Option<f64>,
    excellent_ms: Option<f64>,
    snare_note: Option<u8>,
    snare_channel: Option<u8>,
    welcome_dismissed: Option<bool>,
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
    expected_beat: Option<f64>,
    actual_beat: Option<f64>,
    judgement: Judgement,
    error_ms: f64,
    excellent: bool,
    event_id: Option<String>,
}

fn exercise_beats(exercise: &Exercise) -> f64 {
    let last = exercise.hits.iter().map(|hit| hit.beat).fold(0.0, f64::max);
    ((last + 0.25) / exercise.meter_numerator as f64)
        .ceil()
        .max(1.0)
        * exercise.meter_numerator as f64
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
    beat_ms: f64,
    exercise_length: f64,
    latency_ms: f64,
) -> Vec<LiveJudgement> {
    summary
        .results
        .into_iter()
        .filter(|result| match result.expected_beat {
            Some(expected_beat) if result.judgement == Judgement::Missed => {
                now_ms >= session_start_ms + expected_beat * beat_ms + result.window_ms
            }
            Some(_) => true,
            None => result.observed_ms.is_some(),
        })
        .map(|result| LiveJudgement {
            expected_beat: result.expected_beat.map(|beat| beat % exercise_length),
            actual_beat: result.observed_ms.map(|observed_ms| {
                ((observed_ms - latency_ms - session_start_ms) / beat_ms).max(0.0)
                    % exercise_length
            }),
            judgement: result.judgement,
            error_ms: result.error_ms.unwrap_or(0.0),
            excellent: result.excellent,
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

#[component]
fn App() -> impl IntoView {
    let exercise = RwSignal::new(paradiddle());
    let imported_bytes = RwSignal::new(Vec::<u8>::new());
    let imported_name = RwSignal::new(String::new());
    let phase = RwSignal::new("idle".to_string());
    let bpm = RwSignal::new(60.0);
    let count_in = RwSignal::new(1u8);
    let subdivision = RwSignal::new(1u8);
    let timbre = RwSignal::new("woodblock".to_string());
    let metronome = RwSignal::new(true);
    let click_volume = RwSignal::new(0.48);
    let snare_volume = RwSignal::new(0.72);
    let snare_muted = RwSignal::new(false);
    let loops = RwSignal::new(1u8);
    let follow = RwSignal::new("continuous".to_string());
    let progress = RwSignal::new(-1.0);
    let run_start_ms = RwSignal::new(0.0);
    let observed = RwSignal::new(Vec::<ObservedHit>::new());
    let live = RwSignal::new(Vec::<LiveJudgement>::new());
    let feedback = RwSignal::new("Ready when you are".to_string());
    let audio_state = RwSignal::new("Audio unlocks on first tap".to_string());
    let input_mode = RwSignal::new(InputSource::Demo);
    let result = RwSignal::new(None::<SessionRecord>);
    let history = RwSignal::new(Vec::<SessionRecord>::new());
    let comparison = RwSignal::new(None::<i16>);
    let settings_open = RwSignal::new(false);
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
    let on_time_ms = RwSignal::new(35.0);
    let excellent_ms = RwSignal::new(15.0);

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
            follow: Some(follow.get_untracked()),
            latency_ms: Some(latency_ms.get_untracked()),
            on_time_ms: Some(on_time_ms.get_untracked()),
            excellent_ms: Some(excellent_ms.get_untracked()),
            snare_note: Some(snare_note.get_untracked()),
            snare_channel: Some(snare_channel.get_untracked()),
            welcome_dismissed: Some(!welcome.get_untracked()),
        })
    });

    let finish_run: Rc<dyn Fn()> = Rc::new(move || {
        phase.set("finishing".into());
        let expected = expanded_hits(&exercise.get_untracked(), loops.get_untracked());
        let summary = score_session_with_config(
            &expected,
            &observed.get_untracked(),
            bpm.get_untracked(),
            run_start_ms.get_untracked(),
            latency_ms.get_untracked(),
            ScoringConfig {
                on_time_ms: on_time_ms.get_untracked(),
                excellent_ms: excellent_ms.get_untracked(),
            },
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
        comparison.set(
            previous
                .and_then(|item| Some(current.summary.score? as i16 - item.summary.score? as i16)),
        );
        if let Ok(value) = serde_wasm_bindgen::to_value(&current) {
            spawn_local(async move {
                let _ = JsFuture::from(save_session(value)).await;
            });
        }
        history.update(|items| items.push(current.clone()));
        result.set(Some(current));
        phase.set("complete".into());
        feedback.set("Run complete — saved on this device".into());
    });

    let frame_holder: Rc<RefCell<Option<Closure<dyn FnMut(f64, f64, f64)>>>> =
        Rc::new(RefCell::new(None));
    let done_holder: Rc<RefCell<Option<Closure<dyn FnMut()>>>> = Rc::new(RefCell::new(None));
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
            let total = exercise_beats(&exercise.get_untracked()) * loops.get_untracked() as f64;
            let options=json_compatible(&serde_json::json!({"bpm":bpm.get_untracked(),"countInBars":count_bars,"meter":exercise.get_untracked().meter_numerator,"totalBeats":total,"offsetBeat":offset,"subdivision":subdivision.get_untracked(),"metronome":metronome.get_untracked(),"timbre":timbre.get_untracked()})).unwrap_or(JsValue::NULL);
            let frame = Closure::wrap(Box::new(move |beat: f64, now: f64, start: f64| {
                progress.set(beat);
                let session_start = start - offset * 60_000.0 / bpm.get_untracked();
                run_start_ms.set(session_start);
                if beat >= 0.0 && phase.get_untracked() == "count_in" {
                    phase.set("running".into());
                    feedback.set("Listen, breathe, play".into());
                } else if beat < 0.0 && phase.get_untracked() == "count_in" {
                    feedback.set(format!("Count-in · {}", (-beat).ceil().max(1.0) as u8));
                }
                let base = exercise_beats(&exercise.get_untracked());
                let local = if beat >= 0.0 { beat % base } else { beat };
                let mut judgement_values = JsValue::UNDEFINED;
                if beat >= 0.0 && now - last_judgement_ms.get() >= 30.0 {
                    last_judgement_ms.set(now);
                    let expected = expanded_hits(&exercise.get_untracked(), loops.get_untracked());
                    let summary = score_session_with_config(
                        &expected,
                        &observed.get_untracked(),
                        bpm.get_untracked(),
                        session_start,
                        latency_ms.get_untracked(),
                        ScoringConfig {
                            on_time_ms: on_time_ms.get_untracked(),
                            excellent_ms: excellent_ms.get_untracked(),
                        },
                    );
                    let beat_ms = 60_000.0 / bpm.get_untracked();
                    let previous_misses = live
                        .get_untracked()
                        .iter()
                        .filter(|item| item.judgement == Judgement::Missed)
                        .count();
                    let visible = visible_judgements(
                        summary,
                        now,
                        session_start,
                        beat_ms,
                        base,
                        latency_ms.get_untracked(),
                    );
                    let misses = visible
                        .iter()
                        .filter(|item| item.judgement == Judgement::Missed)
                        .count();
                    if misses > previous_misses {
                        feedback.set("Missed · keep the pulse".into());
                    }
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
                    Ok(value) => {
                        if let Ok(info) = serde_wasm_bindgen::from_value::<serde_json::Value>(value)
                        {
                            let state = info
                                .get("state")
                                .and_then(|v| v.as_str())
                                .unwrap_or("ready");
                            audio_state.set(if state == "fallback" {
                                "Recorded sample failed; temporary fallback active".into()
                            } else {
                                "Audio ready · Tone clock".into()
                            });
                        }
                    }
                    Err(error) => {
                        phase.set("error".into());
                        let detail = error.as_string().unwrap_or_else(|| format!("{error:?}"));
                        audio_state.set(format!("Audio failed: {detail}"));
                    }
                }
            });
        })
    };

    let save_for_hit = save_current_settings.clone();
    let hit_input: Rc<dyn Fn(f64, Instrument, u8, InputSource, Option<u8>, Option<u8>)> = Rc::new(
        move |timestamp, instrument, velocity, source, raw_note, raw_channel| {
            if !snare_muted.get_untracked() && !play_snare(velocity) {
                audio_state
                    .set("Audio is locked — tap Preview or Start once to monitor drums".into());
            }
            input_mode.set(source);
            if !calibration_targets.get_untracked().is_empty() {
                let targets = calibration_targets.get_untracked();
                let used = calibration_errors.get_untracked().len();
                if let Some(target) = targets.get(used) {
                    let error = timestamp - *target;
                    calibration_errors.update(|items| items.push(error));
                    feedback.set(format!("Calibration tap {}/8 · {error:+.0} ms", used + 1));
                    if used + 1 >= 8 {
                        let mut errors = calibration_errors.get_untracked();
                        errors.sort_by(|a, b| a.total_cmp(b));
                        let median = (errors[3] + errors[4]) / 2.0;
                        latency_ms.set(median);
                        calibration_targets.set(vec![]);
                        feedback.set(format!("Calibration saved: {median:+.0} ms input offset"));
                        save_for_hit();
                    }
                }
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
            let beat_ms = 60_000.0 / bpm.get_untracked();
            let session_start = run_start_ms.get_untracked();
            let summary = score_session_with_config(
                &expected,
                &observed.get_untracked(),
                bpm.get_untracked(),
                session_start,
                latency_ms.get_untracked(),
                ScoringConfig {
                    on_time_ms: on_time_ms.get_untracked(),
                    excellent_ms: excellent_ms.get_untracked(),
                },
            );
            let base = exercise_beats(&exercise.get_untracked());
            let visible = visible_judgements(
                summary,
                timestamp,
                session_start,
                beat_ms,
                base,
                latency_ms.get_untracked(),
            );
            let event_id = format!("{timestamp:.3}");
            if let Some(result) = visible
                .iter()
                .find(|item| item.event_id.as_deref() == Some(event_id.as_str()))
            {
                let label = if result.excellent {
                    "Excellent"
                } else {
                    match result.judgement {
                        Judgement::OnTime => "Hit",
                        Judgement::Early => "Early",
                        Judgement::Late => "Late",
                        Judgement::WrongInstrument => "Wrong drum",
                        Judgement::Extra => "Extra hit",
                        _ => "Hit",
                    }
                };
                feedback.set(if matches!(
                    result.judgement,
                    Judgement::OnTime | Judgement::Early | Judgement::Late
                ) {
                    format!("{label} · {:+.0} ms", result.error_ms)
                } else {
                    label.into()
                });
            }
            live.set(visible);
        },
    );

    let file_callback: Rc<Closure<dyn FnMut(Uint8Array, String, String)>> = Rc::new(Closure::wrap(
        Box::new(move |bytes: Uint8Array, name: String, error: String| {
            if !error.is_empty() {
                import_error.set(Some(error));
                return;
            }
            let data = bytes.to_vec();
            match parse_midi(&data, &name, None) {
                Ok(parsed) => {
                    bpm.set(parsed.tempo_bpm.clamp(30.0, 240.0).round());
                    import_error.set(None);
                    imported_bytes.set(data.clone());
                    imported_name.set(name.clone());
                    let metadata = serde_wasm_bindgen::to_value(&parsed).unwrap_or(JsValue::NULL);
                    let bytes_copy = Uint8Array::from(data.as_slice());
                    let hash = parsed.content_hash.clone();
                    spawn_local(async move {
                        let _ =
                            JsFuture::from(save_import(&hash, &name, &bytes_copy, metadata)).await;
                    });
                    exercise.set(parsed);
                    phase.set("idle".into());
                    feedback.set("MIDI imported — review sticking and press Start".into());
                }
                Err(err) => import_error.set(Some(err.to_string())),
            }
        }) as Box<dyn FnMut(Uint8Array, String, String)>,
    ));

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
                event.timestamp.unwrap_or_else(js_sys::Date::now),
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
                render_score(value, &follow.get());
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
    spawn_local(async move {
        if let Ok(value) = JsFuture::from(load_sessions()).await {
            if let Ok(items) = serde_wasm_bindgen::from_value::<Vec<SessionRecord>>(value) {
                history.set(items);
            }
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
                    if let Some(v) = saved.follow {
                        follow.set(v);
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
                Err(error) => feedback.set(format!("Saved settings could not load: {error}")),
            },
            Err(error) => feedback.set(format!("Saved settings storage failed: {error:?}")),
        }
    });

    let key_launch = launch_run.clone();
    let key_hit = hit_input.clone();
    let keyboard = Closure::wrap(Box::new(move |event: web_sys::KeyboardEvent| {
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
                        feedback.set("Paused".into());
                    }
                    "paused" => key_launch(progress.get_untracked().max(0.0), 0),
                    "idle" | "complete" => {
                        observed.set(vec![]);
                        live.set(vec![]);
                        result.set(None);
                        key_launch(0.0, count_in.get_untracked());
                    }
                    _ => {}
                }
            }
            "r" => {
                event.prevent_default();
                stop_audio();
                observed.set(vec![]);
                live.set(vec![]);
                result.set(None);
                progress.set(-1.0);
                key_launch(0.0, count_in.get_untracked());
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
            feedback.set("Paused while this tab is in the background".into());
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
    let pad_hit = hit_input.clone();
    let connect_cb = midi_callback.clone();
    let connect_action: Rc<dyn Fn()> = {
        let callback = connect_cb.clone();
        Rc::new(move || {
            midi_status.set("Requesting permission…".into());
            let audio_promise = ensure_audio();
            let midi_promise = connect_midi(callback.as_ref().as_ref().unchecked_ref());
            spawn_local(async move {
                match JsFuture::from(audio_promise).await {
                    Ok(value) => {
                        let state = serde_wasm_bindgen::from_value::<serde_json::Value>(value)
                            .ok()
                            .and_then(|value| {
                                value
                                    .get("state")
                                    .and_then(serde_json::Value::as_str)
                                    .map(str::to_owned)
                            })
                            .unwrap_or_else(|| "ready".into());
                        audio_state.set(if state == "fallback" {
                            "Recorded sample failed; temporary fallback active".into()
                        } else {
                            "Audio ready · MIDI snare enabled".into()
                        });
                    }
                    Err(error) => {
                        let detail = error.as_string().unwrap_or_else(|| format!("{error:?}"));
                        audio_state.set(format!("Audio failed: {detail}"));
                    }
                }
                let _ = JsFuture::from(midi_promise).await;
            });
        })
    };
    let choose_cb = file_callback.clone();
    let save_tempo_down = save_current_settings.clone();
    let save_tempo_input = save_current_settings.clone();
    let save_tempo_up = save_current_settings.clone();
    let save_welcome = save_current_settings.clone();
    let save_metronome = save_current_settings.clone();
    let save_countin = save_current_settings.clone();
    let save_subdivision = save_current_settings.clone();
    let save_timbre = save_current_settings.clone();
    let save_click_volume = save_current_settings.clone();
    let save_snare_mute = save_current_settings.clone();
    let save_snare_volume = save_current_settings.clone();
    let save_loops = save_current_settings.clone();
    let save_follow = save_current_settings.clone();
    let save_cal_reset = save_current_settings.clone();
    let save_cal_manual = save_current_settings.clone();
    let save_on_time = save_current_settings.clone();
    let save_excellent = save_current_settings.clone();
    view! {
      <main class=move || if phase.get()=="complete"{"app results-active"}else{"app"}>
        <header class="topbar">
          <a class="brand" href="#" aria-label="Rudiment home"><span class="brand-mark">"R"</span><span>"Rudiment"</span></a>
          <div class="exercise-heading"><span class="eyebrow">"CURRENT EXERCISE"</span><strong>{move || exercise.get().title}</strong><small>"4/4 · sticking and accents"</small></div>
          <div class="tempo-control"><button aria-label="Decrease tempo" on:click=move |_|{bpm.update(|v|*v=(*v-1.0).max(30.0));save_tempo_down();}>"−"</button><label><input aria-label="Tempo in BPM" type="number" min="30" max="240" prop:value=move || bpm.get().round().to_string() on:input=move |event|{if let Ok(value)=event_target_value(&event).parse::<f64>(){bpm.set(value.clamp(30.0,240.0));save_tempo_input();}}/><span>"BPM"</span></label><button aria-label="Increase tempo" on:click=move |_|{bpm.update(|v|*v=(*v+1.0).min(240.0));save_tempo_up();}>"+"</button></div>
          <button class=move || if selected_midi.get().is_some(){"status-pill connected"}else{"status-pill"} aria-label="MIDI connection" on:click={let connect=connect_action.clone();move |_|connect()}><span class="status-dot"></span>{move || midi_status.get()}</button>
          <button class="icon-button" aria-label="Open settings" on:click=move |_|settings_open.set(true)>"⚙"</button>
        </header>

          <section class=move || if phase.get()=="complete"{"workspace hidden"}else{"workspace"}>
              <aside class=move || if welcome.get(){"welcome"}else{"welcome hidden"}><div><strong>"A paradiddle is ready."</strong><span>"Press J or tap the pad for snare hits. Space pauses; R restarts."</span></div><button aria-label="Dismiss welcome hint" on:click=move |_|{welcome.set(false);save_welcome();}>"Got it"</button></aside>
            <section id="score-card" class="score-card">
              <div class="score-meta"><div><span class="mode-badge">{move || if selected_midi.get().is_some(){"MIDI INPUT"}else{"DEMO INPUT"}}</span><span>{move || if selected_midi.get().is_some(){"Incoming kit events use your saved mapping."}else{"Try without drums — press J or tap the pad."}}</span></div><div class="score-actions"><button on:click={let callback=choose_cb.clone();move |_|choose_midi(callback.as_ref().as_ref().unchecked_ref())}>"Import MIDI"</button><button on:click={let callback=choose_cb.clone();move |_|load_bundled("public/exercises/single-paradiddle.mid",callback.as_ref().as_ref().unchecked_ref())}>"Load bundled .mid"</button></div></div>
              <div id="notation" class="notation" aria-live="off"></div>
              <div class=move || if exercise.get().warnings.is_empty()&&exercise.get().track_names.len()<2{"warning-row hidden"}else{"warning-row"}><span>{move || exercise.get().warnings.join(" · ")}</span><label class={move || if exercise.get().track_names.len()>1&&!imported_bytes.get().is_empty(){"track-picker"}else{"track-picker hidden"}}>"MIDI track"<select prop:value=move || exercise.get().selected_track.to_string() on:change=move |event|{let index=event_target_value(&event).parse::<usize>().unwrap_or(0);if let Ok(parsed)=parse_midi(&imported_bytes.get_untracked(),&imported_name.get_untracked(),Some(index)){exercise.set(parsed);}}>{move || exercise.get().track_names.into_iter().enumerate().map(|(index,name)|view!{<option value=index.to_string()>{name}</option>}).collect_view()}</select></label></div>
              <div class=move || if import_error.get().is_some(){"error-row"}else{"error-row hidden"}>{move || import_error.get().unwrap_or_default()}</div>
              <div class="position-row"><span>{move || {let current=(progress.get().max(0.0)/exercise.get().meter_numerator as f64).floor() as usize+1;format!("BAR {current} / {}",(exercise_beats(&exercise.get())*loops.get() as f64/exercise.get().meter_numerator as f64) as usize)}}</span><div class="timeline"><i style=move || format!("width:{:.2}%",(progress.get().max(0.0)/(exercise_beats(&exercise.get())*loops.get() as f64)*100.0).clamp(0.0,100.0))></i></div><span>{move || format!("{} / {} LOOP",((progress.get().max(0.0)/exercise_beats(&exercise.get())).floor() as u8+1).min(loops.get()),loops.get())}</span></div>
            </section>
            <section class="feedback-strip" aria-live="polite"><div class=move || format!("feedback-orb {}",feedback.get().split_whitespace().next().unwrap_or("ready").to_lowercase())></div><strong>{move || feedback.get()}</strong><span>{move || {let hits=observed.get().len();format!("{hits} received")}}</span><span>{move || audio_state.get()}</span></section>
            <section class="practice-controls">
              <button class=move || if matches!(phase.get().as_str(),"running"|"count_in"){"pad is-hit"}else{"pad"} aria-label="Play snare hit" on:pointerdown={let pad_hit=pad_hit.clone();move |_|pad_hit(web_sys::window().unwrap().performance().unwrap().now(),Instrument::Snare,108,InputSource::Demo,None,None)}><span>"SNARE"</span><kbd>"J"</kbd></button>
              <div class="transport">
                <div class="transport-secondary"><button on:click=move |_|{stop_audio();phase.set("idle".into());progress.set(-1.0);feedback.set("Stopped — results were not saved".into());}>"■ Stop"</button><button on:click={let launch=start_click.clone();move |_|{stop_audio();observed.set(vec![]);live.set(vec![]);result.set(None);progress.set(-1.0);launch(0.0,count_in.get_untracked());}}>"↻ Restart"</button></div>
                <button class="primary start-button" on:click={let launch=start_click.clone();move |_|match phase.get_untracked().as_str(){"running"|"count_in"=>{stop_audio();phase.set("paused".into());feedback.set("Paused".into());},"paused"=>launch(progress.get_untracked().max(0.0),0),_=>{observed.set(vec![]);live.set(vec![]);result.set(None);input_mode.set(if selected_midi.get_untracked().is_some(){InputSource::Midi}else{InputSource::Demo});launch(0.0,count_in.get_untracked());}}}>{move || match phase.get().as_str(){"running"|"count_in"=>"Pause","paused"=>"Resume",_=>"Start practice"}}<small>{move || if count_in.get()>0{format!("{} bar count-in",count_in.get())}else{"No count-in".into()}}</small></button>
              </div>
              <button class="preview-card" on:click=move |_|{audio_state.set("Loading acoustic snare…".into());let promise=preview_snare();spawn_local(async move{if JsFuture::from(promise).await.is_ok(){audio_state.set("Acoustic snare ready".into());}});}><span>"Preview"</span><strong>"Acoustic snare"</strong><em>"▶"</em></button>
            </section>
          </section>

            <section class=move || if phase.get()=="complete"{"results-view"}else{"results-view hidden"}><div class="results-kicker">"SESSION SAVED · LOCALLY"</div><h1>"Good work."</h1><p>{move || result.get().map(|session|format!("{} at {:.0} BPM · {} input",session.exercise_title,session.bpm,match session.input_mode{InputSource::Demo=>"Demo",InputSource::Midi=>"MIDI"})).unwrap_or_default()}</p>
              <div class="score-ring" style=move || format!("--score:{}",result.get().and_then(|session|session.summary.score).unwrap_or(0))><div><strong>{move || result.get().and_then(|session|session.summary.score).unwrap_or(0)}</strong><span>"/ 100"</span></div></div>
              <div class="result-metrics"><article><span>"HITS"</span><strong>{move || result.get().map(|session|format!("{} / {}",session.summary.matched,session.summary.expected)).unwrap_or_else(||"—".into())}</strong></article><article><span>"MEAN ERROR"</span><strong>{move || result.get().and_then(|session|session.summary.mean_abs_error_ms).map(|value|format!("{value:.0} ms")).unwrap_or_else(||"—".into())}</strong></article><article><span>"TIMING BIAS"</span><strong>{move || result.get().and_then(|session|session.summary.signed_mean_error_ms).map(|value|if value<0.0{format!("{:.0} ms early",value.abs())}else{format!("{value:.0} ms late")}).unwrap_or_else(||"—".into())}</strong></article><article><span>"MISSED"</span><strong>{move || result.get().map(|session|session.summary.missed.to_string()).unwrap_or_else(||"—".into())}</strong></article><article><span>"EXTRAS"</span><strong>{move || result.get().map(|session|session.summary.extras.to_string()).unwrap_or_else(||"—".into())}</strong></article><article><span>"WRONG DRUM"</span><strong>{move || result.get().map(|session|session.summary.wrong_instrument.to_string()).unwrap_or_else(||"—".into())}</strong></article></div>
              <div class="comparison-card"><div><span>"COMPARABLE PREVIOUS RUN"</span><strong>{move || comparison.get().map(|delta|if delta>0{format!("+{delta} points") }else if delta<0{format!("{delta} points")}else{"No change".into()}).unwrap_or_else(||"First comparable run".into())}</strong></div><div><span>"PERSONAL BEST"</span><strong>{move || result.get().map(|session|history.get().iter().filter(|item|item.exercise_hash==session.exercise_hash&&(item.bpm-session.bpm).abs()<0.01&&item.input_mode==session.input_mode&&item.scoring_version==session.scoring_version&&(item.tolerance_ms-session.tolerance_ms).abs()<0.01).filter_map(|item|item.summary.score).max().unwrap_or(session.summary.score.unwrap_or(0)).to_string()).unwrap_or_else(||"—".into())}</strong></div><small>"Same exercise version, tempo, input mode, tolerance and scoring v1 only."</small></div>
              <div class="result-actions"><button class="primary" on:click={let launch=start_click.clone();move |_|{observed.set(vec![]);live.set(vec![]);result.set(None);progress.set(-1.0);launch(0.0,count_in.get_untracked());}}>"Practise again"</button><button on:click=move |_|{phase.set("idle".into());result.set(None);progress.set(-1.0);}>"Back to score"</button></div>
            </section>

        <aside class=move || if settings_open.get(){"settings-drawer open"}else{"settings-drawer"} aria-hidden=move || (!settings_open.get()).to_string()>
          <div class="drawer-head"><div><span class="eyebrow">"PRACTICE SETUP"</span><h2>"Sound & drums"</h2></div><button aria-label="Close settings" on:click=move |_|settings_open.set(false)>"×"</button></div>
          <div class="drawer-scroll">
            <section><h3>"Metronome"</h3><label class="toggle-row"><span>"Click during exercise"</span><input type="checkbox" prop:checked=move || metronome.get() on:change=move |event|{metronome.set(event_target_checked(&event));save_metronome();}/></label>
              <label>"Count-in"<select prop:value=move || count_in.get().to_string() on:change=move |event|{count_in.set(event_target_value(&event).parse().unwrap_or(1));save_countin();}><option value="0">"Off"</option><option value="1">"1 bar"</option><option value="2">"2 bars"</option></select></label>
              <label>"Subdivision"<select prop:value=move || subdivision.get().to_string() on:change=move |event|{subdivision.set(event_target_value(&event).parse().unwrap_or(1));save_subdivision();}><option value="1">"Quarter notes"</option><option value="2">"Eighth notes"</option><option value="4">"Sixteenth notes"</option></select></label>
              <label>"Click timbre"<div class="inline-select"><select prop:value=move || timbre.get() on:change=move |event|{timbre.set(event_target_value(&event));save_timbre();}><option value="woodblock">"Warm woodblock"</option><option value="crisp">"Crisp click"</option><option value="cowbell">"Cowbell"</option></select><button on:click=move |_|{let promise=preview_click(&timbre.get_untracked());spawn_local(async move{let _=JsFuture::from(promise).await;});}>"Preview"</button></div></label>
              <label>"Metronome volume"<input type="range" min="0" max="1" step="0.01" prop:value=move || click_volume.get() on:input=move |event|{let value=event_target_value(&event).parse().unwrap_or(0.48);click_volume.set(value);set_volumes(if snare_muted.get_untracked(){0.0}else{snare_volume.get_untracked()},value);save_click_volume();}/></label>
            </section>
            <section><h3>"Reference snare"</h3><label class="toggle-row"><span>"Play app snare"<small>"Mute this if your drum module is audible."</small></span><input type="checkbox" prop:checked=move || !snare_muted.get() on:change=move |event|{snare_muted.set(!event_target_checked(&event));set_volumes(if snare_muted.get_untracked(){0.0}else{snare_volume.get_untracked()},click_volume.get_untracked());save_snare_mute();}/></label><label>"Snare volume"<input type="range" min="0" max="1" step="0.01" prop:value=move || snare_volume.get() on:input=move |event|{let value=event_target_value(&event).parse().unwrap_or(0.72);snare_volume.set(value);set_volumes(if snare_muted.get_untracked(){0.0}else{value},click_volume.get_untracked());save_snare_volume();}/></label></section>
            <section><h3>"Exercise"</h3><label>"Loops"<select prop:value=move || loops.get().to_string() on:change=move |event|{loops.set(event_target_value(&event).parse().unwrap_or(1));save_loops();}><option value="1">"1 run"</option><option value="2">"2 loops"</option><option value="4">"4 loops"</option></select></label><label>"Score following"<select prop:value=move || follow.get() on:change=move |event|{follow.set(event_target_value(&event));save_follow();}><option value="continuous">"Continuous horizontal"</option><option value="page">"Bar/page follow"</option></select></label></section>
            <section><h3>"MIDI drums"</h3><p class="help">"Use the MIDI status control in the top bar to connect or rescan. Rudiment never uploads your events."</p><p class="midi-state">{move || midi_status.get()}</p>
              <label class=move || if midi_ports.get().is_empty(){"hidden"}else{""}>"Input device"<select prop:value=move || selected_midi.get().unwrap_or_default() on:change=move |event|select_midi(&event_target_value(&event))>{move || midi_ports.get().into_iter().map(|port|view!{<option value=port.id.clone()>{format!("{} · {}",port.manufacturer,port.name)}</option>}).collect_view()}</select></label>
              <div class="learn-card"><strong>{move || if midi_learn.get(){"Hit your snare now…"}else{"Quick MIDI Learn"}}</strong><span>{move || format!("Current snare: note {}, channel {}",snare_note.get(),snare_channel.get())}</span><button on:click=move |_|midi_learn.set(true)>{move || if midi_learn.get(){"Listening…"}else{"Learn snare"}}</button></div>
              <details><summary>"MIDI diagnostics"</summary><div class="diagnostics">{move || if diagnostics.get().is_empty(){view!{<p>"No MIDI messages received yet."</p>}.into_any()}else{diagnostics.get().into_iter().map(|line|view!{<code>{line}</code>}).collect_view().into_any()}}</div></details>
            </section>
            <section><h3>"Latency calibration"</h3><p class="help">"Tap your snare or J with each of eight clicks. This estimates input offset; it cannot remove all external hardware latency."</p><strong class="cal-value">{move || format!("{:+.0} ms",latency_ms.get())}</strong><div class="button-row"><button on:click=move |_|{calibration_errors.set(vec![]);feedback.set("Calibration: tap with each click".into());let promise=start_calibration();spawn_local(async move{if let Ok(value)=JsFuture::from(promise).await{let array=Array::from(&value);calibration_targets.set(array.iter().filter_map(|item|item.as_f64()).collect());}});}>"Start 8-tap calibration"</button><button on:click=move |_|{latency_ms.set(0.0);calibration_targets.set(vec![]);save_cal_reset();}>"Reset"</button></div><label>"Manual offset"<input type="range" min="-200" max="200" step="1" prop:value=move || latency_ms.get() on:input=move |event|{latency_ms.set(event_target_value(&event).parse().unwrap_or(0.0));save_cal_manual();}/></label></section>
            <section><h3>"Timing feedback"</h3><p class="help">"Excellent is the tight green flash. Hits inside the wider on-time window remain green; earlier hits are blue and later hits yellow."</p><label>{move || format!("Excellent window · ±{:.0} ms",excellent_ms.get())}<input type="range" min="5" max="25" step="1" prop:value=move || excellent_ms.get() on:input=move |event|{let value=event_target_value(&event).parse::<f64>().unwrap_or(15.0).min(on_time_ms.get_untracked());excellent_ms.set(value);save_excellent();}/></label><label>{move || format!("On-time window · ±{:.0} ms",on_time_ms.get())}<input type="range" min="20" max="60" step="1" prop:value=move || on_time_ms.get() on:input=move |event|{let value=event_target_value(&event).parse::<f64>().unwrap_or(35.0);on_time_ms.set(value);if excellent_ms.get_untracked()>value{excellent_ms.set(value);}save_on_time();}/></label></section>
            <section><h3>"Local data"</h3><p class="help">{move || format!("{} completed session(s) stored only in this browser.",history.get().len())}</p><div class="button-row"><button on:click=move |_|{let promise=export_history();spawn_local(async move{let _=JsFuture::from(promise).await;});}>"Export JSON"</button><button class="danger" on:click=move |_|{if web_sys::window().and_then(|window|window.confirm_with_message("Clear all Rudiment practice history from this browser?").ok()).unwrap_or(false){let promise=clear_history();spawn_local(async move{if JsFuture::from(promise).await.is_ok(){history.set(vec![]);}});}}>"Clear history"</button></div><button class="wide-button" on:click={move |_|{let promise=reset_local_setup();bpm.set(60.0);count_in.set(1);subdivision.set(1);timbre.set("woodblock".into());metronome.set(true);click_volume.set(0.48);snare_volume.set(0.72);snare_muted.set(false);loops.set(1);follow.set("continuous".into());latency_ms.set(0.0);on_time_ms.set(35.0);excellent_ms.set(15.0);snare_note.set(38);snare_channel.set(10);selected_midi.set(None);midi_status.set("Not connected".into());spawn_local(async move{let _=JsFuture::from(promise).await;});}}>"Reset settings & mappings"</button></section>
          </div>
        </aside><button class=move || if settings_open.get(){"drawer-backdrop"}else{"drawer-backdrop hidden"} aria-label="Close settings" on:click=move |_|settings_open.set(false)></button>
      </main>
    }
}

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(App);
}
