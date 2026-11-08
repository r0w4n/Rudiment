//! Pure musical domain model for Rudiment. This crate deliberately has no web/UI dependencies.

use midly::{MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

pub const SCORING_VERSION: &str = "v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Instrument {
    Snare,
    Kick,
    ClosedHat,
    OpenHat,
    PedalHat,
    Ride,
    Crash,
    Tom,
    OtherPercussion,
    Unknown,
}

impl Instrument {
    pub fn from_gm(note: u8) -> Self {
        match note {
            35 | 36 => Self::Kick,
            37 | 38 | 40 => Self::Snare,
            42 => Self::ClosedHat,
            44 => Self::PedalHat,
            46 => Self::OpenHat,
            51 | 53 | 59 => Self::Ride,
            41 | 43 | 45 | 47 | 48 | 50 => Self::Tom,
            49 | 52 | 55 | 57 => Self::Crash,
            39 | 54 | 56 | 58 | 60..=81 => Self::OtherPercussion,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Hand {
    L,
    R,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExpectedHit {
    pub id: String,
    pub tick: u64,
    pub beat: f64,
    pub notation_beat: f64,
    pub raw_duration_ticks: Option<u64>,
    pub notation_duration_beats: f64,
    pub instrument: Instrument,
    pub velocity: u8,
    pub accent: bool,
    pub hand: Hand,
    pub channel: u8,
    pub note: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TempoChange {
    pub tick: u64,
    pub beat: f64,
    pub bpm: f64,
}

#[derive(Debug, Clone)]
struct RawMidiNote {
    tick: u64,
    channel: u8,
    note: u8,
    velocity: u8,
    duration_ticks: Option<u64>,
}

fn nearest_written_value(beats: f64) -> f64 {
    [
        4.0,
        3.0,
        2.0,
        1.5,
        1.0,
        0.75,
        2.0 / 3.0,
        0.5,
        0.375,
        1.0 / 3.0,
        0.25,
        1.0 / 6.0,
        0.125,
    ]
        .into_iter()
        .find(|value| *value <= beats + 0.001)
        .unwrap_or(0.25)
}

fn rhythmic_grid(positions: &[f64]) -> f64 {
    let intervals: Vec<f64> = positions
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .filter(|interval| *interval > 0.001)
        .collect();
    let candidates = [
        4.0,
        3.0,
        2.0,
        1.5,
        1.0,
        0.75,
        2.0 / 3.0,
        0.5,
        0.375,
        1.0 / 3.0,
        0.25,
        1.0 / 6.0,
        0.125,
    ];
    candidates
        .into_iter()
        .filter_map(|grid| {
            let errors: Vec<f64> = intervals
                .iter()
                .map(|interval| {
                    let multiple = (*interval / grid).round().max(1.0);
                    (interval - multiple * grid).abs()
                })
                .collect();
            (errors.iter().copied().fold(0.0, f64::max) <= 0.06)
                .then(|| (grid, errors.iter().sum::<f64>()))
        })
        .min_by(|(a, a_error), (b, b_error)| {
            a_error.total_cmp(b_error).then_with(|| b.total_cmp(a))
        })
        .map(|(grid, _)| grid)
        .unwrap_or(0.25)
}

fn measure_beats(numerator: u8, denominator: u8) -> f64 {
    numerator as f64 * 4.0 / denominator.max(1) as f64
}

/// Infers written percussion values from onset spacing and bar context, never short MIDI gate time.
fn infer_notation(hits: &mut [ExpectedHit], meter: (u8, u8)) {
    let bar_beats = measure_beats(meter.0, meter.1);
    let mut positions: Vec<f64> = hits.iter().map(|hit| hit.beat).collect();
    positions.sort_by(f64::total_cmp);
    positions.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    let grid = rhythmic_grid(&positions);
    for hit in hits.iter_mut() {
        hit.notation_beat = (hit.beat / grid).round() * grid;
    }
    positions = hits.iter().map(|hit| hit.notation_beat).collect();
    positions.sort_by(f64::total_cmp);
    positions.dedup_by(|a, b| (*a - *b).abs() < 0.001);

    let mut counts: BTreeMap<i32, usize> = BTreeMap::new();
    for pair in positions.windows(2) {
        let delta = pair[1] - pair[0];
        if delta > 0.001 && delta <= bar_beats {
            let value = nearest_written_value(delta);
            *counts.entry((value * 24.0).round() as i32).or_default() += 1;
        }
    }
    let pulse = counts
        .into_iter()
        .max_by(|(a, ac), (b, bc)| ac.cmp(bc).then_with(|| b.cmp(a)))
        .map(|(twenty_fourths, _)| twenty_fourths as f64 / 24.0)
        .unwrap_or(1.0);

    let mut interval_counts: BTreeMap<i32, usize> = BTreeMap::new();
    for pair in positions.windows(2) {
        let delta = pair[1] - pair[0];
        if delta > 0.001 && delta <= bar_beats {
            *interval_counts
                .entry((nearest_written_value(delta) * 24.0).round() as i32)
                .or_default() += 1;
        }
    }
    for hit in hits.iter_mut() {
        let bar_end = ((hit.notation_beat / bar_beats).floor() + 1.0) * bar_beats;
        let next = positions
            .iter()
            .copied()
            .find(|position| *position > hit.notation_beat + 0.001)
            .unwrap_or(bar_end)
            .min(bar_end);
        let gap = (next - hit.notation_beat).max(0.25);
        let written_gap = nearest_written_value(gap);
        let repeated_mixed_value = interval_counts
            .get(&((written_gap * 24.0).round() as i32))
            .copied()
            .unwrap_or_default()
            >= 2;
        let infer_gap_as_note = gap <= pulse * 1.5 || repeated_mixed_value;
        hit.notation_duration_beats =
            nearest_written_value(if infer_gap_as_note { gap } else { pulse }).min(gap);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Exercise {
    pub id: String,
    pub title: String,
    pub content_hash: String,
    pub schema_version: u16,
    pub ticks_per_beat: u16,
    pub tempo_bpm: f64,
    #[serde(default)]
    pub tempo_map: Vec<TempoChange>,
    pub meter_numerator: u8,
    pub meter_denominator: u8,
    pub hits: Vec<ExpectedHit>,
    pub track_names: Vec<String>,
    pub selected_track: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Error)]
pub enum MidiError {
    #[error("The MIDI file is malformed or truncated: {0}")]
    Parse(String),
    #[error("SMPTE timing is not supported; use a PPQ MIDI file")]
    Smpte,
    #[error("This file contains too many events (limit 100,000)")]
    TooManyEvents,
    #[error("No playable note-on events were found")]
    NoNotes,
}

pub fn parse_midi(
    bytes: &[u8],
    filename: &str,
    selected_track: Option<usize>,
) -> Result<Exercise, MidiError> {
    if bytes.len() > 8 * 1024 * 1024 {
        return Err(MidiError::TooManyEvents);
    }
    let smf = Smf::parse(bytes).map_err(|e| MidiError::Parse(e.to_string()))?;
    let ppq = match smf.header.timing {
        Timing::Metrical(v) => v.as_int(),
        _ => return Err(MidiError::Smpte),
    };
    let event_count: usize = smf.tracks.iter().map(Vec::len).sum();
    if event_count > 100_000 {
        return Err(MidiError::TooManyEvents);
    }

    let mut tempo_events = BTreeMap::new();
    let mut meter = (4u8, 4u8);
    let mut names = vec![];
    let mut candidates: Vec<Vec<RawMidiNote>> = vec![];
    let mut lyrics_by_track: Vec<BTreeMap<u64, Vec<String>>> = vec![];
    for (ti, track) in smf.tracks.iter().enumerate() {
        let mut tick = 0u64;
        let mut notes: Vec<RawMidiNote> = vec![];
        let mut pending: HashMap<(u8, u8), Vec<usize>> = HashMap::new();
        let mut lyrics: BTreeMap<u64, Vec<String>> = BTreeMap::new();
        let mut name = format!("Track {}", ti + 1);
        for ev in track {
            tick += ev.delta.as_int() as u64;
            match ev.kind {
                TrackEventKind::Meta(MetaMessage::TrackName(raw)) => {
                    name = String::from_utf8_lossy(raw).to_string()
                }
                TrackEventKind::Meta(MetaMessage::Tempo(v)) => {
                    tempo_events.entry(tick).or_insert(v.as_int());
                }
                TrackEventKind::Meta(MetaMessage::TimeSignature(n, d, _, _)) => {
                    meter = (n, 2u8.saturating_pow(d as u32))
                }
                TrackEventKind::Meta(MetaMessage::Lyric(raw)) => {
                    let text = String::from_utf8_lossy(raw).trim().to_uppercase();
                    if text == "R" || text == "L" {
                        lyrics.entry(tick).or_default().push(text);
                    }
                }
                TrackEventKind::Midi {
                    channel,
                    message: MidiMessage::NoteOn { key, vel },
                } if vel.as_int() > 0 => {
                    let index = notes.len();
                    notes.push(RawMidiNote {
                        tick,
                        channel: channel.as_int(),
                        note: key.as_int(),
                        velocity: vel.as_int(),
                        duration_ticks: None,
                    });
                    pending
                        .entry((channel.as_int(), key.as_int()))
                        .or_default()
                        .push(index);
                }
                TrackEventKind::Midi {
                    channel,
                    message: MidiMessage::NoteOff { key, .. },
                }
                | TrackEventKind::Midi {
                    channel,
                    message: MidiMessage::NoteOn { key, vel: _ },
                } => {
                    if let Some(indices) = pending.get_mut(&(channel.as_int(), key.as_int())) {
                        if !indices.is_empty() {
                            let index = indices.remove(0);
                            notes[index].duration_ticks =
                                Some(tick.saturating_sub(notes[index].tick));
                        }
                    }
                }
                _ => {}
            }
        }
        names.push(name);
        candidates.push(notes);
        lyrics_by_track.push(lyrics);
    }
    tempo_events.entry(0).or_insert(500_000);
    let tempo_map: Vec<TempoChange> = tempo_events
        .into_iter()
        .map(|(tick, micros_per_quarter)| TempoChange {
            tick,
            beat: tick as f64 / ppq as f64,
            bpm: 60_000_000.0 / micros_per_quarter as f64,
        })
        .collect();
    let tempo = tempo_map[0].bpm;
    let track_index = selected_track
        .filter(|i| *i < candidates.len())
        .unwrap_or_else(|| {
            candidates
                .iter()
                .enumerate()
                .max_by_key(|(i, ns)| {
                    let percussion = ns
                        .iter()
                        .filter(|note| {
                            note.channel == 9
                                || Instrument::from_gm(note.note) != Instrument::Unknown
                        })
                        .count();
                    (percussion, ns.len(), usize::MAX - *i)
                })
                .map(|(i, _)| i)
                .unwrap_or(0)
        });
    let raw_notes = candidates.get(track_index).cloned().unwrap_or_default();
    if raw_notes.is_empty() {
        return Err(MidiError::NoNotes);
    }
    let mut warnings = vec![];
    if candidates.iter().filter(|n| !n.is_empty()).count() > 1 && selected_track.is_none() {
        warnings.push(format!(
            "Multiple note tracks found; selected ‘{}’. Choose another track if needed.",
            names[track_index]
        ));
    }
    let lyrics = &lyrics_by_track[track_index];
    let mut tick_counts: HashMap<u64, usize> = HashMap::new();
    for note in &raw_notes {
        *tick_counts.entry(note.tick).or_default() += 1;
    }
    let mut hits = vec![];
    let mut missing = 0usize;
    let mut ambiguous = 0usize;
    for (i, raw) in raw_notes.into_iter().enumerate() {
        let RawMidiNote {
            tick,
            channel,
            note,
            velocity,
            duration_ticks,
        } = raw;
        let hand = match (
            lyrics.get(&tick),
            tick_counts.get(&tick).copied().unwrap_or(1),
        ) {
            (Some(v), 1) if v.len() == 1 && v[0] == "R" => Hand::R,
            (Some(v), 1) if v.len() == 1 && v[0] == "L" => Hand::L,
            (Some(_), _) => {
                ambiguous += 1;
                Hand::Unknown
            }
            _ => {
                missing += 1;
                Hand::Unknown
            }
        };
        let instrument = Instrument::from_gm(note);
        if instrument == Instrument::Unknown {
            warnings.push(format!(
                "MIDI note {note} is not in the supported GM percussion map."
            ));
        }
        hits.push(ExpectedHit {
            id: format!("n{i}"),
            tick,
            beat: tick as f64 / ppq as f64,
            notation_beat: tick as f64 / ppq as f64,
            raw_duration_ticks: duration_ticks,
            notation_duration_beats: 0.25,
            instrument,
            velocity,
            accent: velocity >= 112,
            hand,
            channel,
            note,
        });
    }
    warnings.sort();
    warnings.dedup();
    infer_notation(&mut hits, meter);
    let shifted = hits
        .iter()
        .filter(|hit| (hit.notation_beat - hit.beat).abs() > 0.04)
        .count();
    if shifted > 0 {
        warnings.push(format!("{shifted} onset(s) were quantised by more than 0.04 beats for notation; playback timing is preserved."));
    }
    if ambiguous > 0 {
        warnings.push(format!(
            "{ambiguous} note(s) had ambiguous same-tick lyric sticking and show as ·."
        ));
    }
    if missing > 0 {
        warnings.push(format!(
            "{missing} note(s) have no aligned R/L lyric and show as ·."
        ));
    }
    let hash = format!("{:x}", Sha256::digest(bytes));
    let title = filename
        .strip_suffix(".midi")
        .or_else(|| filename.strip_suffix(".mid"))
        .or_else(|| filename.strip_suffix(".MIDI"))
        .or_else(|| filename.strip_suffix(".MID"))
        .unwrap_or(filename)
        .to_string();
    Ok(Exercise {
        id: format!("import-{}", &hash[..12]),
        title,
        content_hash: hash,
        schema_version: 1,
        ticks_per_beat: ppq,
        tempo_bpm: tempo,
        tempo_map,
        meter_numerator: meter.0,
        meter_denominator: meter.1,
        hits,
        track_names: names,
        selected_track: track_index,
        warnings,
    })
}

pub fn paradiddle() -> Exercise {
    let hands = [
        Hand::R,
        Hand::L,
        Hand::R,
        Hand::R,
        Hand::L,
        Hand::R,
        Hand::L,
        Hand::L,
    ];
    let mut hits = vec![];
    for bar in 0..4 {
        for (step, hand) in hands.iter().enumerate() {
            let beat = bar as f64 * 4.0 + step as f64 * 0.5;
            hits.push(ExpectedHit {
                id: format!("p-{bar}-{step}"),
                tick: (beat * 480.0) as u64,
                beat,
                notation_beat: beat,
                raw_duration_ticks: None,
                notation_duration_beats: 0.5,
                instrument: Instrument::Snare,
                velocity: if step == 0 || step == 4 { 120 } else { 88 },
                accent: step == 0 || step == 4,
                hand: *hand,
                channel: 9,
                note: 38,
            });
        }
    }
    Exercise {
        id: "single-paradiddle".into(),
        title: "Single Paradiddle".into(),
        content_hash: "builtin-paradiddle-v1".into(),
        schema_version: 1,
        ticks_per_beat: 480,
        tempo_bpm: 60.0,
        tempo_map: vec![TempoChange {
            tick: 0,
            beat: 0.0,
            bpm: 60.0,
        }],
        meter_numerator: 4,
        meter_denominator: 4,
        hits,
        track_names: vec!["Paradiddle".into()],
        selected_track: 0,
        warnings: vec![],
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservedHit {
    pub timestamp_ms: f64,
    pub instrument: Instrument,
    pub velocity: u8,
    pub source: InputSource,
    pub raw_note: Option<u8>,
    pub raw_channel: Option<u8>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InputSource {
    Demo,
    Midi,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Judgement {
    OnTime,
    Early,
    Late,
    Missed,
    Extra,
    WrongInstrument,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ScoringConfig {
    pub on_time_ms: f64,
    pub excellent_ms: f64,
}
impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            on_time_ms: 35.0,
            excellent_ms: 15.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HitResult {
    pub expected_id: Option<String>,
    pub expected_beat: Option<f64>,
    pub observed_ms: Option<f64>,
    pub error_ms: Option<f64>,
    pub window_ms: f64,
    pub instrument: Instrument,
    pub judgement: Judgement,
    #[serde(default)]
    pub excellent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSummary {
    pub score: Option<u8>,
    pub matched: usize,
    pub expected: usize,
    pub missed: usize,
    pub extras: usize,
    pub wrong_instrument: usize,
    pub mean_abs_error_ms: Option<f64>,
    pub signed_mean_error_ms: Option<f64>,
    pub scoring_version: String,
    pub results: Vec<HitResult>,
}

pub fn matching_window_ms(hits: &[ExpectedHit], idx: usize, bpm: f64) -> f64 {
    matching_window_ms_with_tempo_map(hits, idx, bpm, &[])
}

pub fn elapsed_ms_at_beat(beat: f64, bpm: f64, tempo_map: &[TempoChange]) -> f64 {
    if tempo_map.is_empty() {
        return beat.max(0.0) * 60_000.0 / bpm.max(1.0);
    }
    let scale = bpm.max(1.0) / tempo_map[0].bpm.max(1.0);
    let mut cursor = 0.0;
    let mut current_bpm = tempo_map[0].bpm * scale;
    let mut elapsed = 0.0;
    for change in tempo_map.iter().skip(1) {
        if change.beat >= beat {
            break;
        }
        if change.beat > cursor {
            elapsed += (change.beat - cursor) * 60_000.0 / current_bpm.max(1.0);
        }
        cursor = change.beat;
        current_bpm = change.bpm * scale;
    }
    elapsed + (beat - cursor).max(0.0) * 60_000.0 / current_bpm.max(1.0)
}

pub fn beat_at_elapsed_ms(elapsed_ms: f64, bpm: f64, tempo_map: &[TempoChange]) -> f64 {
    if tempo_map.is_empty() {
        return elapsed_ms.max(0.0) * bpm.max(1.0) / 60_000.0;
    }
    let scale = bpm.max(1.0) / tempo_map[0].bpm.max(1.0);
    let mut cursor = 0.0;
    let mut current_bpm = tempo_map[0].bpm * scale;
    let mut remaining = elapsed_ms.max(0.0);
    for change in tempo_map.iter().skip(1) {
        let segment_beats = change.beat - cursor;
        if segment_beats <= 0.0 {
            current_bpm = change.bpm * scale;
            continue;
        }
        let segment_ms = segment_beats * 60_000.0 / current_bpm.max(1.0);
        if remaining <= segment_ms {
            return cursor + remaining * current_bpm / 60_000.0;
        }
        remaining -= segment_ms;
        cursor = change.beat;
        current_bpm = change.bpm * scale;
    }
    cursor + remaining * current_bpm / 60_000.0
}

pub fn matching_window_ms_with_tempo_map(
    hits: &[ExpectedHit],
    idx: usize,
    bpm: f64,
    tempo_map: &[TempoChange],
) -> f64 {
    let current = hits[idx].beat;
    let distances = hits
        .iter()
        .enumerate()
        .filter(|(j, h)| *j != idx && h.beat != current)
        .map(|(_, h)| {
            (elapsed_ms_at_beat(h.beat, bpm, tempo_map)
                - elapsed_ms_at_beat(current, bpm, tempo_map))
            .abs()
        })
        .filter(|d| *d > 0.0);
    let nearest = distances.fold(f64::INFINITY, f64::min);
    if nearest.is_finite() {
        120.0_f64.min(0.4 * nearest).max(35.0)
    } else {
        120.0
    }
}

/// Deterministic nearest-time, same-instrument, one-to-one matcher and Score v1.
pub fn score_session(
    expected: &[ExpectedHit],
    observed: &[ObservedHit],
    bpm: f64,
    start_ms: f64,
    latency_ms: f64,
) -> SessionSummary {
    score_session_with_config(
        expected,
        observed,
        bpm,
        start_ms,
        latency_ms,
        ScoringConfig::default(),
    )
}

pub fn score_session_with_config(
    expected: &[ExpectedHit],
    observed: &[ObservedHit],
    bpm: f64,
    start_ms: f64,
    latency_ms: f64,
    config: ScoringConfig,
) -> SessionSummary {
    score_session_with_tempo_map(
        expected,
        observed,
        bpm,
        start_ms,
        latency_ms,
        config,
        &[],
    )
}

pub fn score_session_with_tempo_map(
    expected: &[ExpectedHit],
    observed: &[ObservedHit],
    bpm: f64,
    start_ms: f64,
    latency_ms: f64,
    config: ScoringConfig,
    tempo_map: &[TempoChange],
) -> SessionSummary {
    if expected.is_empty() {
        return SessionSummary {
            score: None,
            matched: 0,
            expected: 0,
            missed: 0,
            extras: observed.len(),
            wrong_instrument: 0,
            mean_abs_error_ms: None,
            signed_mean_error_ms: None,
            scoring_version: SCORING_VERSION.into(),
            results: vec![],
        };
    }
    let mut used = vec![false; observed.len()];
    let mut results = vec![];
    let mut errors = vec![];
    let mut timing_sum = 0.0;
    for (i, exp) in expected.iter().enumerate() {
        let target = start_ms + elapsed_ms_at_beat(exp.beat, bpm, tempo_map);
        let window = matching_window_ms_with_tempo_map(expected, i, bpm, tempo_map);
        let best = observed
            .iter()
            .enumerate()
            .filter(|(oi, o)| !used[*oi] && o.instrument == exp.instrument)
            .map(|(oi, o)| (oi, o.timestamp_ms - latency_ms - target))
            .filter(|(_, e)| e.abs() <= window)
            .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()).then(a.0.cmp(&b.0)));
        if let Some((oi, error)) = best {
            used[oi] = true;
            errors.push(error);
            timing_sum += (1.0 - error.abs() / window).clamp(0.0, 1.0);
            results.push(HitResult {
                expected_id: Some(exp.id.clone()),
                expected_beat: Some(exp.beat),
                observed_ms: Some(observed[oi].timestamp_ms),
                error_ms: Some(error),
                window_ms: window,
                instrument: exp.instrument,
                judgement: if error.abs() <= config.on_time_ms {
                    Judgement::OnTime
                } else if error < 0.0 {
                    Judgement::Early
                } else {
                    Judgement::Late
                },
                excellent: error.abs() <= config.excellent_ms,
            });
        } else {
            results.push(HitResult {
                expected_id: Some(exp.id.clone()),
                expected_beat: Some(exp.beat),
                observed_ms: None,
                error_ms: None,
                window_ms: window,
                instrument: exp.instrument,
                judgement: Judgement::Missed,
                excellent: false,
            });
        }
    }
    let mut wrong = 0usize;
    let mut extras = 0usize;
    for (oi, obs) in observed.iter().enumerate().filter(|(oi, _)| !used[*oi]) {
        let near_other = expected.iter().enumerate().any(|(i, exp)| {
            exp.instrument != obs.instrument
                && (obs.timestamp_ms
                    - latency_ms
                    - (start_ms + elapsed_ms_at_beat(exp.beat, bpm, tempo_map)))
                    .abs()
                    <= matching_window_ms_with_tempo_map(expected, i, bpm, tempo_map)
        });
        let judgement = if near_other {
            wrong += 1;
            Judgement::WrongInstrument
        } else {
            extras += 1;
            Judgement::Extra
        };
        results.push(HitResult {
            expected_id: None,
            expected_beat: None,
            observed_ms: Some(obs.timestamp_ms),
            error_ms: None,
            window_ms: 0.0,
            instrument: obs.instrument,
            judgement,
            excellent: false,
        });
        let _ = oi;
    }
    let n = expected.len();
    let matched = errors.len();
    let missed = n - matched;
    let hit_rate = matched as f64 / n as f64;
    let timing_quality = timing_sum / n as f64;
    let extra_penalty = 0.10 * (extras as f64 / n as f64).min(1.0);
    let score = (100.0 * (0.70 * hit_rate + 0.30 * timing_quality - extra_penalty).clamp(0.0, 1.0))
        .round() as u8;
    let mean_abs = (!errors.is_empty())
        .then(|| errors.iter().map(|v| v.abs()).sum::<f64>() / errors.len() as f64);
    let signed = (!errors.is_empty()).then(|| errors.iter().sum::<f64>() / errors.len() as f64);
    SessionSummary {
        score: Some(score),
        matched,
        expected: n,
        missed,
        extras,
        wrong_instrument: wrong,
        mean_abs_error_ms: mean_abs,
        signed_mean_error_ms: signed,
        scoring_version: SCORING_VERSION.into(),
        results,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn obs(ms: f64, instrument: Instrument) -> ObservedHit {
        ObservedHit {
            timestamp_ms: ms,
            instrument,
            velocity: 100,
            source: InputSource::Demo,
            raw_note: None,
            raw_channel: None,
        }
    }

    #[test]
    fn perfect_is_100_and_deterministic() {
        let e = paradiddle();
        let o: Vec<_> = e
            .hits
            .iter()
            .map(|h| obs(h.beat * 1000.0, Instrument::Snare))
            .collect();
        let a = score_session(&e.hits, &o, 60.0, 0.0, 0.0);
        let b = score_session(&e.hits, &o, 60.0, 0.0, 0.0);
        assert_eq!(a.score, Some(100));
        assert_eq!(a, b);
        assert_eq!(a.scoring_version, "v1");
    }
    #[test]
    fn early_late_and_missing_extra_classify() {
        let e = paradiddle();
        let mut o = vec![obs(-60.0, Instrument::Snare), obs(560.0, Instrument::Snare)];
        o.push(obs(9_999.0, Instrument::Snare));
        let s = score_session(&e.hits[..3], &o, 60.0, 0.0, 0.0);
        assert_eq!(s.results[0].judgement, Judgement::Early);
        assert_eq!(s.results[1].judgement, Judgement::Late);
        assert_eq!(s.missed, 1);
        assert_eq!(s.extras, 1);
        assert!(s.score.unwrap() < 100);
    }
    #[test]
    fn wrong_instrument_never_matches() {
        let e = paradiddle();
        let s = score_session(&e.hits[..1], &[obs(0.0, Instrument::Kick)], 60.0, 0.0, 0.0);
        assert_eq!(s.matched, 0);
        assert_eq!(s.wrong_instrument, 1);
    }
    #[test]
    fn close_hits_are_not_double_matched() {
        let e = paradiddle();
        let s = score_session(&e.hits[..2], &[obs(0.0, Instrument::Snare)], 60.0, 0.0, 0.0);
        assert_eq!(s.matched, 1);
        assert_eq!(s.missed, 1);
    }
    #[test]
    fn note_on_zero_is_ignored_and_lyrics_align() {
        let bytes = fixture(false);
        let e = parse_midi(&bytes, "test.mid", None).unwrap();
        assert_eq!(e.hits.len(), 2);
        assert_eq!(e.hits[0].hand, Hand::R);
        assert_eq!(e.hits[1].hand, Hand::L);
    }
    #[test]
    fn corrupt_midi_is_rejected() {
        assert!(parse_midi(b"MThd\0", "bad.mid", None).is_err());
    }
    #[test]
    fn ambiguous_lyrics_are_not_guessed() {
        let e = parse_midi(&fixture(true), "ambiguous.mid", None).unwrap();
        assert!(e.hits.iter().all(|h| h.hand == Hand::Unknown));
        assert!(e.warnings.iter().any(|w| w.contains("ambiguous")));
    }
    #[test]
    fn bundled_paradiddle_has_expected_grid_and_sticking() {
        let bytes = include_bytes!("../../../web/public/exercises/single-paradiddle.mid");
        let e = parse_midi(bytes, "single-paradiddle.mid", None).unwrap();
        assert_eq!(e.hits.len(), 32);
        assert_eq!((e.meter_numerator, e.meter_denominator), (4, 4));
        assert!((e.tempo_bpm - 60.0).abs() < 0.01);
        let expected = [
            Hand::R,
            Hand::L,
            Hand::R,
            Hand::R,
            Hand::L,
            Hand::R,
            Hand::L,
            Hand::L,
        ];
        assert_eq!(
            &e.hits[..8].iter().map(|hit| hit.hand).collect::<Vec<_>>(),
            &expected
        );
        assert!((e.hits[1].beat - 0.5).abs() < f64::EPSILON);
        assert!(e.hits[0].accent);
        assert_eq!(e.hits[0].channel, 9);
    }
    #[test]
    fn smf1_multitrack_uses_absolute_deltas_and_metadata() {
        let bytes = parallel_fixture((3, 2));
        let e = parse_midi(&bytes, "parallel.mid", None).unwrap();
        assert_eq!(e.track_names, vec!["Conductor", "Drums", "Other"]);
        assert_eq!(e.selected_track, 1);
        assert_eq!((e.meter_numerator, e.meter_denominator), (3, 4));
        assert!((e.tempo_bpm - 100.0).abs() < 0.01);
        assert_eq!(e.tempo_map.len(), 2);
        assert!((e.tempo_map[1].beat - 1.0).abs() < f64::EPSILON);
        assert!((e.tempo_map[1].bpm - 150.0).abs() < 0.01);
        assert_eq!(
            e.hits.iter().map(|hit| hit.tick).collect::<Vec<_>>(),
            vec![0, 240, 720]
        );
        assert_eq!(
            e.hits.iter().map(|hit| hit.instrument).collect::<Vec<_>>(),
            vec![Instrument::Kick, Instrument::Snare, Instrument::PedalHat]
        );
        assert!(
            e.warnings
                .iter()
                .any(|warning| warning.contains("Multiple note tracks"))
        );
        let other = parse_midi(&bytes, "parallel.mid", Some(2)).unwrap();
        assert_eq!(other.selected_track, 2);
    }
    #[test]
    fn six_eight_midi_uses_three_quarter_note_beats_per_measure() {
        let exercise = parse_midi(&parallel_fixture((6, 3)), "six-eight.mid", None).unwrap();
        assert_eq!(
            (exercise.meter_numerator, exercise.meter_denominator),
            (6, 8)
        );
        assert!(exercise.hits.iter().all(|hit| hit.notation_beat < 3.0));
    }
    #[test]
    fn final_hit_inside_window_is_not_missed() {
        let e = paradiddle();
        let last = e.hits.last().unwrap().clone();
        let target = last.beat * 1000.0;
        let s = score_session(
            &[last],
            &[obs(target + 34.0, Instrument::Snare)],
            60.0,
            0.0,
            0.0,
        );
        assert_eq!(s.matched, 1);
        assert_eq!(s.missed, 0);
        assert_eq!(s.results[0].judgement, Judgement::OnTime);
    }
    #[test]
    fn quarter_note_onsets_ignore_short_gate_duration() {
        let e = parse_midi(&quarter_fixture(), "EarDrum-reference.mid", None).unwrap();
        assert_eq!(e.hits.len(), 8);
        assert!(e.hits.iter().all(|hit| hit.raw_duration_ticks == Some(110)));
        assert!(
            e.hits
                .iter()
                .all(|hit| (hit.notation_duration_beats - 1.0).abs() < f64::EPSILON)
        );
        assert_eq!(
            e.hits
                .iter()
                .take(4)
                .map(|hit| hit.hand)
                .collect::<Vec<_>>(),
            vec![Hand::R, Hand::L, Hand::R, Hand::L]
        );
        assert_eq!(
            e.hits
                .iter()
                .take(4)
                .map(|hit| hit.beat)
                .collect::<Vec<_>>(),
            vec![0.0, 1.0, 2.0, 3.0]
        );
    }
    fn rhythm_hits(beats: &[f64]) -> Vec<ExpectedHit> {
        beats
            .iter()
            .enumerate()
            .map(|(index, beat)| ExpectedHit {
                id: format!("rhythm-{index}"),
                tick: (*beat * 480.0).round() as u64,
                beat: *beat,
                notation_beat: *beat,
                raw_duration_ticks: Some(20),
                notation_duration_beats: 0.25,
                instrument: Instrument::Snare,
                velocity: 100,
                accent: false,
                hand: Hand::Unknown,
                channel: 9,
                note: 38,
            })
            .collect()
    }
    #[test]
    fn regular_onsets_infer_quarters_eighths_sixteenths_and_triplets() {
        let cases = [
            (vec![0.0, 1.0, 2.0, 3.0], 1.0),
            ((0..8).map(|i| i as f64 * 0.5).collect(), 0.5),
            ((0..16).map(|i| i as f64 * 0.25).collect(), 0.25),
            ((0..12).map(|i| i as f64 / 3.0).collect(), 1.0 / 3.0),
        ];
        for (beats, expected_duration) in cases {
            let mut hits = rhythm_hits(&beats);
            infer_notation(&mut hits, (4, 4));
            assert!(hits.iter().all(|hit| {
                (hit.notation_duration_beats - expected_duration).abs() < 0.001
            }), "expected duration {expected_duration}, got {:?}", hits.iter().map(|hit| hit.notation_duration_beats).collect::<Vec<_>>());
            assert!(hits
                .iter()
                .zip(beats)
                .all(|(hit, beat)| (hit.notation_beat - beat).abs() < 0.001));
        }
    }
    #[test]
    fn mixed_subdivisions_and_genuine_gaps_use_onset_context() {
        let mut mixed = rhythm_hits(&[0.0, 0.5, 1.5, 2.0, 3.0, 3.5]);
        infer_notation(&mut mixed, (4, 4));
        assert_eq!(
            mixed
                .iter()
                .map(|hit| hit.notation_duration_beats)
                .collect::<Vec<_>>(),
            vec![0.5, 1.0, 0.5, 1.0, 0.5, 0.5]
        );

        let mut with_gap = rhythm_hits(&[0.0, 0.5, 1.0, 1.5, 2.0, 4.0]);
        infer_notation(&mut with_gap, (4, 4));
        assert_eq!(with_gap[4].notation_duration_beats, 0.5);
        assert!((4.0 - (with_gap[4].notation_beat + with_gap[4].notation_duration_beats)) > 1.0);
    }
    #[test]
    fn tempo_map_converts_beats_to_and_from_elapsed_time() {
        let tempo_map = [
            TempoChange {
                tick: 0,
                beat: 0.0,
                bpm: 120.0,
            },
            TempoChange {
                tick: 480,
                beat: 1.0,
                bpm: 60.0,
            },
        ];
        assert!((elapsed_ms_at_beat(2.0, 120.0, &tempo_map) - 1500.0).abs() < 0.001);
        assert!((beat_at_elapsed_ms(1500.0, 120.0, &tempo_map) - 2.0).abs() < 0.001);
        assert!((elapsed_ms_at_beat(2.0, 60.0, &tempo_map) - 3000.0).abs() < 0.001);
    }
    #[test]
    fn scoring_uses_tempo_map_times_without_changing_notation_beats() {
        let exercise = rhythm_hits(&[0.0, 1.0, 2.0]);
        let tempo_map = [
            TempoChange {
                tick: 0,
                beat: 0.0,
                bpm: 120.0,
            },
            TempoChange {
                tick: 480,
                beat: 1.0,
                bpm: 60.0,
            },
        ];
        let observed = [
            obs(0.0, Instrument::Snare),
            obs(500.0, Instrument::Snare),
            obs(1500.0, Instrument::Snare),
        ];
        let summary = score_session_with_tempo_map(
            &exercise,
            &observed,
            120.0,
            0.0,
            0.0,
            ScoringConfig::default(),
            &tempo_map,
        );
        assert_eq!(summary.matched, 3);
        assert_eq!(
            summary
                .results
                .iter()
                .map(|result| result.expected_beat.unwrap())
                .collect::<Vec<_>>(),
            vec![0.0, 1.0, 2.0]
        );
    }
    #[test]
    fn drum_map_covers_extended_gm_percussion_and_compound_meter() {
        assert_eq!(Instrument::from_gm(37), Instrument::Snare);
        assert_eq!(Instrument::from_gm(39), Instrument::OtherPercussion);
        assert_eq!(Instrument::from_gm(54), Instrument::OtherPercussion);
        assert_eq!(Instrument::from_gm(80), Instrument::OtherPercussion);
        assert_eq!(Instrument::from_gm(82), Instrument::Unknown);
        assert_eq!(measure_beats(6, 8), 3.0);
        assert_eq!(nearest_written_value(0.75), 0.75);
        assert_eq!(nearest_written_value(1.5), 1.5);
    }
    #[test]
    fn configurable_timing_boundaries_and_simultaneous_hits_are_deterministic() {
        let e = paradiddle();
        let mut simultaneous = vec![e.hits[0].clone(), e.hits[0].clone()];
        simultaneous[1].id = "second".into();
        let observed = vec![obs(10.0, Instrument::Snare), obs(35.0, Instrument::Snare)];
        let config = ScoringConfig {
            on_time_ms: 35.0,
            excellent_ms: 12.0,
        };
        let a = score_session_with_config(&simultaneous, &observed, 60.0, 0.0, 0.0, config);
        let b = score_session_with_config(&simultaneous, &observed, 60.0, 0.0, 0.0, config);
        assert_eq!(a, b);
        assert_eq!(a.matched, 2);
        assert!(a.results[0].excellent);
        assert!(!a.results[1].excellent);
        assert_eq!(a.results[1].judgement, Judgement::OnTime);
    }
    fn fixture(ambiguous: bool) -> Vec<u8> {
        use midly::{
            Format, Header, Smf, TrackEvent, TrackEventKind,
            num::{u4, u7, u15, u24, u28},
        };
        let mut t = vec![
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(500_000))),
            },
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::Lyric(b"R")),
            },
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Midi {
                    channel: u4::new(9),
                    message: MidiMessage::NoteOn {
                        key: u7::new(38),
                        vel: u7::new(100),
                    },
                },
            },
        ];
        if ambiguous {
            t.push(TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Midi {
                    channel: u4::new(9),
                    message: MidiMessage::NoteOn {
                        key: u7::new(38),
                        vel: u7::new(100),
                    },
                },
            });
        } else {
            t.extend([
                TrackEvent {
                    delta: u28::new(480),
                    kind: TrackEventKind::Meta(MetaMessage::Lyric(b"L")),
                },
                TrackEvent {
                    delta: u28::new(0),
                    kind: TrackEventKind::Midi {
                        channel: u4::new(9),
                        message: MidiMessage::NoteOn {
                            key: u7::new(38),
                            vel: u7::new(100),
                        },
                    },
                },
                TrackEvent {
                    delta: u28::new(0),
                    kind: TrackEventKind::Midi {
                        channel: u4::new(9),
                        message: MidiMessage::NoteOn {
                            key: u7::new(38),
                            vel: u7::new(0),
                        },
                    },
                },
            ]);
        }
        t.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        let smf = Smf {
            header: Header {
                format: Format::SingleTrack,
                timing: Timing::Metrical(u15::new(480)),
            },
            tracks: vec![t],
        };
        let mut out = vec![];
        smf.write_std(&mut out).unwrap();
        out
    }
    fn parallel_fixture(meter: (u8, u8)) -> Vec<u8> {
        use midly::{
            Format, Header, Smf, TrackEvent, TrackEventKind,
            num::{u4, u7, u15, u24, u28},
        };
        let conductor = vec![
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::TrackName(b"Conductor")),
            },
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(600_000))),
            },
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::TimeSignature(meter.0, meter.1, 24, 8)),
            },
            TrackEvent {
                delta: u28::new(480),
                kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(400_000))),
            },
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
            },
        ];
        let drums = vec![
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::TrackName(b"Drums")),
            },
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Midi {
                    channel: u4::new(9),
                    message: MidiMessage::NoteOn {
                        key: u7::new(36),
                        vel: u7::new(100),
                    },
                },
            },
            TrackEvent {
                delta: u28::new(240),
                kind: TrackEventKind::Midi {
                    channel: u4::new(9),
                    message: MidiMessage::NoteOn {
                        key: u7::new(38),
                        vel: u7::new(100),
                    },
                },
            },
            TrackEvent {
                delta: u28::new(480),
                kind: TrackEventKind::Midi {
                    channel: u4::new(9),
                    message: MidiMessage::NoteOn {
                        key: u7::new(44),
                        vel: u7::new(100),
                    },
                },
            },
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
            },
        ];
        let other = vec![
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::TrackName(b"Other")),
            },
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Midi {
                    channel: u4::new(0),
                    message: MidiMessage::NoteOn {
                        key: u7::new(60),
                        vel: u7::new(90),
                    },
                },
            },
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
            },
        ];
        let smf = Smf {
            header: Header {
                format: Format::Parallel,
                timing: Timing::Metrical(u15::new(480)),
            },
            tracks: vec![conductor, drums, other],
        };
        let mut out = vec![];
        smf.write_std(&mut out).unwrap();
        out
    }
    fn quarter_fixture() -> Vec<u8> {
        use midly::{
            Format, Header, Smf, TrackEvent, TrackEventKind,
            num::{u4, u7, u15, u24, u28},
        };
        let mut track = vec![
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(750_000))),
            },
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::TimeSignature(4, 2, 24, 8)),
            },
        ];
        for index in 0..8 {
            let lead = if index == 0 { 0 } else { 370 };
            let hand = if index % 2 == 0 {
                b"R".as_slice()
            } else {
                b"L".as_slice()
            };
            track.push(TrackEvent {
                delta: u28::new(lead),
                kind: TrackEventKind::Meta(MetaMessage::Lyric(hand)),
            });
            track.push(TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Midi {
                    channel: u4::new(9),
                    message: MidiMessage::NoteOn {
                        key: u7::new(38),
                        vel: u7::new(100),
                    },
                },
            });
            track.push(TrackEvent {
                delta: u28::new(110),
                kind: TrackEventKind::Midi {
                    channel: u4::new(9),
                    message: MidiMessage::NoteOff {
                        key: u7::new(38),
                        vel: u7::new(0),
                    },
                },
            });
        }
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        let smf = Smf {
            header: Header {
                format: Format::SingleTrack,
                timing: Timing::Metrical(u15::new(480)),
            },
            tracks: vec![track],
        };
        let mut out = vec![];
        smf.write_std(&mut out).unwrap();
        out
    }
}
