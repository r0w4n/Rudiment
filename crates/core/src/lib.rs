//! Pure musical domain model for Rudiment. This crate deliberately has no web/UI dependencies.

use midly::{Format, MetaMessage, MidiMessage, Smf, Timing, TrackEventKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use thiserror::Error;

pub const SCORING_VERSION: &str = "v4";
pub const SCORING_INVALID_HIT_PENALTY: f64 = 50.0;
pub const SCORING_LOCAL_WORK_LIMIT: usize = 16_384;
/// Maximum MIDI file size accepted by the parser and browser import preflight.
pub const MAX_MIDI_FILE_BYTES: usize = 8 * 1024 * 1024;

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
    #[serde(default)]
    pub grace: bool,
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
    for index in 0..hits.len() {
        if hits[index].grace {
            continue;
        }
        let offset = hits[index].beat.rem_euclid(bar_beats);
        let has_leading_grace = hits[..index]
            .iter()
            .rev()
            .any(|candidate| candidate.grace && hits[index].beat - candidate.beat <= 0.2);
        if offset <= 0.2 && has_leading_grace {
            hits[index].notation_beat = hits[index].beat - offset;
        }
    }
    let mut positions: Vec<f64> = hits
        .iter()
        .filter(|hit| !hit.grace)
        .map(|hit| hit.notation_beat)
        .collect();
    if positions.is_empty() {
        positions = hits.iter().map(|hit| hit.beat).collect();
    }
    positions.sort_by(f64::total_cmp);
    positions.dedup_by(|a, b| (*a - *b).abs() < 0.001);
    let grid = rhythmic_grid(&positions);
    for hit in hits.iter_mut().filter(|hit| !hit.grace) {
        let triplet_position = (hit.notation_beat * 3.0).round() / 3.0;
        hit.notation_beat = if (triplet_position - hit.notation_beat).abs() <= 0.02 {
            triplet_position
        } else {
            (hit.notation_beat / grid).round() * grid
        };
    }
    positions = hits
        .iter()
        .filter(|hit| !hit.grace)
        .map(|hit| hit.notation_beat)
        .collect();
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
    for hit in hits.iter_mut().filter(|hit| !hit.grace) {
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

    // Grace strokes remain part of the audible/scored timeline, but are
    // engraved against the principal note they ornament. This keeps their
    // deliberately early MIDI timestamp without allowing it to distort the
    // rhythmic grid used for the main voice.
    for index in 0..hits.len() {
        if !hits[index].grace {
            continue;
        }
        let principal = hits[index + 1..]
            .iter()
            .find(|candidate| !candidate.grace && candidate.beat - hits[index].beat <= 0.25);
        if let Some(principal) = principal {
            hits[index].notation_beat = principal.notation_beat;
            hits[index].notation_duration_beats = 0.125;
        }
    }
}

fn mark_grace_notes(hits: &mut [ExpectedHit], ticks_per_beat: u16) {
    let maximum_gate = (u64::from(ticks_per_beat) / 12).max(1);
    for index in 0..hits.len() {
        let instrument = hits[index].instrument;
        let beat = hits[index].beat;
        let velocity = hits[index].velocity;
        let gate = hits[index].raw_duration_ticks;
        let has_principal = hits[index + 1..]
            .iter()
            .take_while(|candidate| candidate.beat - beat <= 0.2)
            .any(|candidate| {
                candidate.instrument == instrument
                    && candidate.beat > beat + 0.001
                    && candidate.velocity >= velocity.saturating_add(20)
            });
        hits[index].grace =
            velocity <= 64 && gate.is_some_and(|gate| gate <= maximum_gate) && has_principal;
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
    #[serde(default)]
    pub playable_tracks: Vec<PlayableTrack>,
    pub selected_track: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PlayableTrack {
    /// The original SMF track index. This deliberately differs from the
    /// position in `playable_tracks` when a conductor track is present.
    pub index: usize,
    pub name: String,
    pub instruments: Vec<Instrument>,
}

#[derive(Debug, Error)]
pub enum MidiError {
    #[error("The MIDI file is malformed or truncated: {0}")]
    Parse(String),
    #[error("MIDI files larger than {} MB are not supported", MAX_MIDI_FILE_BYTES / (1024 * 1024))]
    FileTooLarge,
    #[error("SMPTE timing is not supported; use a PPQ MIDI file")]
    Smpte,
    #[error("MIDI PPQ resolution must be greater than zero")]
    InvalidPpq,
    #[error("MIDI tempo must be greater than zero")]
    InvalidTempo,
    #[error("MIDI time signature must have a non-zero numerator and a supported denominator")]
    InvalidMeter,
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
    if bytes.len() > MAX_MIDI_FILE_BYTES {
        return Err(MidiError::FileTooLarge);
    }
    let smf = Smf::parse(bytes).map_err(|e| MidiError::Parse(e.to_string()))?;
    let ppq = match smf.header.timing {
        Timing::Metrical(v) if v.as_int() > 0 => v.as_int(),
        Timing::Metrical(_) => return Err(MidiError::InvalidPpq),
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
    let mut invalid_tempo = false;
    let mut invalid_meter = false;
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
                    if v.as_int() == 0 {
                        invalid_tempo = true;
                    } else {
                        tempo_events.entry(tick).or_insert(v.as_int());
                    }
                }
                TrackEventKind::Meta(MetaMessage::TimeSignature(n, d, _, _)) => {
                    if n == 0 || d > 7 {
                        invalid_meter = true;
                    } else {
                        meter = (n, 2u8.pow(d as u32));
                    }
                }
                TrackEventKind::Meta(MetaMessage::Lyric(raw))
                | TrackEventKind::Meta(MetaMessage::Text(raw))
                | TrackEventKind::Meta(MetaMessage::Marker(raw))
                | TrackEventKind::Meta(MetaMessage::CuePoint(raw)) => {
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
                    if let Some(indices) = pending.get_mut(&(channel.as_int(), key.as_int()))
                        && !indices.is_empty()
                    {
                        let index = indices.remove(0);
                        notes[index].duration_ticks = Some(tick.saturating_sub(notes[index].tick));
                    }
                }
                _ => {}
            }
        }
        names.push(name);
        candidates.push(notes);
        lyrics_by_track.push(lyrics);
    }
    if invalid_tempo {
        return Err(MidiError::InvalidTempo);
    }
    if invalid_meter {
        return Err(MidiError::InvalidMeter);
    }
    // Format-0 files place every channel in one physical track. When that
    // track contains channel 10 percussion, melodic channels are accompaniment
    // rather than additional drum instruments and must not enter notation or
    // scoring.
    if smf.header.format == Format::SingleTrack
        && let Some(notes) = candidates.first_mut()
        && notes.iter().any(|note| note.channel == 9)
    {
        notes.retain(|note| note.channel == 9);
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
    let has_channel_ten = candidates.iter().flatten().any(|note| note.channel == 9);
    let playable_tracks = candidates
        .iter()
        .enumerate()
        .filter(|(_, notes)| {
            !notes.is_empty() && (!has_channel_ten || notes.iter().any(|note| note.channel == 9))
        })
        .map(|(index, notes)| {
            let mut instruments = notes
                .iter()
                .map(|note| Instrument::from_gm(note.note))
                .filter(|instrument| *instrument != Instrument::Unknown)
                .collect::<Vec<_>>();
            instruments.sort_by_key(|instrument| *instrument as u8);
            instruments.dedup();
            PlayableTrack {
                index,
                name: names[index].clone(),
                instruments,
            }
        })
        .collect::<Vec<_>>();
    let track_index = selected_track
        .filter(|i| *i < candidates.len())
        .unwrap_or_else(|| {
            candidates
                .iter()
                .enumerate()
                .max_by_key(|(i, ns)| {
                    let channel_ten = ns.iter().filter(|note| note.channel == 9).count();
                    let recognised = ns
                        .iter()
                        .filter(|note| Instrument::from_gm(note.note) != Instrument::Unknown)
                        .count();
                    (channel_ten, recognised, ns.len(), usize::MAX - *i)
                })
                .map(|(i, _)| i)
                .unwrap_or(0)
        });
    let raw_notes = candidates.get(track_index).cloned().unwrap_or_default();
    if raw_notes.is_empty() {
        return Err(MidiError::NoNotes);
    }
    let mut warnings = vec![];
    if playable_tracks.len() > 1 && selected_track.is_none() {
        warnings.push(format!(
            "Multiple note tracks found; selected ‘{}’. Choose another track if needed.",
            names[track_index]
        ));
    }
    // Exporters commonly split sticking text between the drum track and a
    // conductor/annotation track. Local annotations win at the same tick, but
    // annotations on other tracks fill any local gaps.
    let local_lyrics = &lyrics_by_track[track_index];
    let mut merged_lyrics = local_lyrics.clone();
    for (index, track) in lyrics_by_track.iter().enumerate() {
        if index == track_index {
            continue;
        }
        for (tick, values) in track {
            if !local_lyrics.contains_key(tick) {
                merged_lyrics
                    .entry(*tick)
                    .or_default()
                    .extend(values.iter().cloned());
            }
        }
    }
    let lyrics = &merged_lyrics;
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
            accent: velocity >= 105,
            grace: false,
            hand,
            channel,
            note,
        });
    }
    warnings.sort();
    warnings.dedup();
    mark_grace_notes(&mut hits, ppq);
    infer_notation(&mut hits, meter);
    let shifted = hits
        .iter()
        .filter(|hit| !hit.grace)
        .filter(|hit| (hit.notation_beat - hit.beat).abs() > 0.04)
        .filter(|hit| {
            !hits.iter().any(|candidate| {
                candidate.grace
                    && (candidate.notation_beat - hit.notation_beat).abs() < 0.001
                    && hit.beat - candidate.beat <= 0.2
            })
        })
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
    let hash: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
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
        playable_tracks,
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
                grace: false,
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
        playable_tracks: vec![PlayableTrack {
            index: 0,
            name: "Paradiddle".into(),
            instruments: vec![Instrument::Snare],
        }],
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
    Perfect,
    Good,
    /// Read-only compatibility for sessions saved by scoring v1.
    OnTime,
    Early,
    Late,
    Missed,
    Extra,
    WrongInstrument,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct ScoringConfig {
    pub perfect_ms: f64,
    pub good_ms: f64,
    pub match_ms: f64,
    /// Retained for source compatibility with scoring v2. Scoring v3 uses
    /// fixed, judgement-level points rather than an interpolated curve.
    pub good_floor_score: f64,
    /// Retained for source compatibility with scoring v2. Scoring v3 uses
    /// fixed, judgement-level points rather than an interpolated curve.
    pub match_edge_score: f64,
}
pub const DEFAULT_PERFECT_MS: f64 = 20.0;
pub const DEFAULT_GOOD_MS: f64 = 60.0;
pub const DEFAULT_MATCH_MS: f64 = 100.0;

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            perfect_ms: DEFAULT_PERFECT_MS,
            good_ms: DEFAULT_GOOD_MS,
            match_ms: DEFAULT_MATCH_MS,
            good_floor_score: 95.0,
            match_edge_score: 60.0,
        }
    }
}

impl ScoringConfig {
    fn boundaries(self) -> (f64, f64, f64) {
        let perfect = self.perfect_ms.max(0.0);
        let good = self.good_ms.max(perfect);
        let matched = self.match_ms.max(good);
        (perfect, good, matched)
    }

    pub fn classify(self, timing_error_ms: f64) -> Judgement {
        let (perfect, good, matched) = self.boundaries();
        let magnitude = timing_error_ms.abs();
        if magnitude <= perfect {
            Judgement::Perfect
        } else if magnitude <= good {
            Judgement::Good
        } else if magnitude <= matched && timing_error_ms < 0.0 {
            Judgement::Early
        } else if magnitude <= matched {
            Judgement::Late
        } else {
            Judgement::Missed
        }
    }

    pub fn note_score(self, timing_error_ms: f64) -> f64 {
        match self.classify(timing_error_ms) {
            Judgement::Perfect => 100.0,
            Judgement::Good => 95.0,
            Judgement::Early | Judgement::Late => 60.0,
            Judgement::OnTime => 95.0,
            Judgement::Missed | Judgement::Extra | Judgement::WrongInstrument => 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HitResult {
    pub expected_id: Option<String>,
    pub expected_beat: Option<f64>,
    #[serde(default)]
    pub expected_timestamp_ms: Option<f64>,
    pub observed_ms: Option<f64>,
    #[serde(default)]
    pub actual_timestamp_ms: Option<f64>,
    pub error_ms: Option<f64>,
    pub window_ms: f64,
    pub instrument: Instrument,
    #[serde(default)]
    pub midi_note: Option<u8>,
    pub judgement: Judgement,
    /// Read-only compatibility for the v1 "Excellent" flag.
    #[serde(default)]
    pub excellent: bool,
    #[serde(default)]
    pub note_score: f64,
    #[serde(default)]
    pub missed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSummary {
    pub score: Option<f64>,
    pub matched: usize,
    pub expected: usize,
    pub missed: usize,
    pub extras: usize,
    pub wrong_instrument: usize,
    pub mean_abs_error_ms: Option<f64>,
    pub signed_mean_error_ms: Option<f64>,
    #[serde(default)]
    pub timing_consistency_ms: Option<f64>,
    pub scoring_version: String,
    pub results: Vec<HitResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoringFeedback {
    pub result: HitResult,
    pub confirmed: bool,
}

#[derive(Debug, Clone, Error, PartialEq)]
pub enum IncrementalScoringError {
    #[error("scoring event time must be finite")]
    NonFiniteTime,
    #[error(
        "MIDI event at {event_ms:.3} ms arrived behind the confirmed scoring frontier at {watermark_ms:.3} ms"
    )]
    LateDelivery { event_ms: f64, watermark_ms: f64 },
    #[error(
        "local scoring workload exceeded its safe limit ({expected} expected × {observed} observed events)"
    )]
    Overloaded { expected: usize, observed: usize },
}

#[derive(Debug, Clone)]
struct IncrementalExpected {
    hit: ExpectedHit,
    target_ms: f64,
    confirmed: bool,
}

#[derive(Debug, Clone)]
struct IncrementalObserved {
    hit: ObservedHit,
    calibrated_ms: f64,
    arrival: usize,
    consumed: bool,
    confirmed_invalid: bool,
}

#[derive(Debug, Clone, Default)]
struct LocalPlan {
    utility_points: i32,
    matches: usize,
    exact_notes: usize,
    total_error_ns: i64,
    pairs: Vec<(usize, usize)>,
}

fn prefer_local_plan(candidate: &LocalPlan, current: &LocalPlan) -> bool {
    candidate.utility_points > current.utility_points
        || (candidate.utility_points == current.utility_points
            && (candidate.matches > current.matches
                || (candidate.matches == current.matches
                    && (candidate.exact_notes > current.exact_notes
                        || (candidate.exact_notes == current.exact_notes
                            && (candidate.total_error_ns < current.total_error_ns
                                || (candidate.total_error_ns == current.total_error_ns
                                    && candidate.pairs < current.pairs)))))))
}

/// Authoritative, event-time scoring engine for scoring policy v4.
///
/// Expected notes are finalized in musical-time order once their inclusive
/// matching window has closed. At each frontier the engine optimizes the
/// actual score numerator over only the hits delivered by that frontier and
/// the expected notes those hits can reach. This prevents overlapping windows
/// from creating an unbounded reassignment chain.
#[derive(Debug, Clone)]
pub struct IncrementalScorer {
    expected: Vec<IncrementalExpected>,
    expected_order: Vec<usize>,
    observed: Vec<IncrementalObserved>,
    expected_results: Vec<Option<HitResult>>,
    invalid_results: Vec<HitResult>,
    confirmed_log: Vec<HitResult>,
    feedback_cursor: usize,
    bpm: f64,
    origin_ms: f64,
    config: ScoringConfig,
    tempo_map: Vec<TempoChange>,
    watermark_ms: f64,
    next_expected: usize,
    next_arrival: usize,
    revision: u64,
    overloaded: bool,
}

impl IncrementalScorer {
    pub fn new(
        expected: &[ExpectedHit],
        bpm: f64,
        origin_ms: f64,
        config: ScoringConfig,
        tempo_map: &[TempoChange],
    ) -> Self {
        let expected = expected
            .iter()
            .cloned()
            .map(|hit| IncrementalExpected {
                target_ms: origin_ms + elapsed_ms_at_beat(hit.beat, bpm, tempo_map),
                hit,
                confirmed: false,
            })
            .collect::<Vec<_>>();
        let mut expected_order = (0..expected.len()).collect::<Vec<_>>();
        expected_order.sort_by(|left, right| {
            expected[*left]
                .target_ms
                .total_cmp(&expected[*right].target_ms)
                .then(left.cmp(right))
        });
        let expected_results = vec![None; expected.len()];
        Self {
            expected,
            expected_order,
            observed: Vec::new(),
            expected_results,
            invalid_results: Vec::new(),
            confirmed_log: Vec::new(),
            feedback_cursor: 0,
            bpm,
            origin_ms,
            config,
            tempo_map: tempo_map.to_vec(),
            watermark_ms: f64::NEG_INFINITY,
            next_expected: 0,
            next_arrival: 0,
            revision: 0,
            overloaded: false,
        }
    }

    pub fn origin_ms(&self) -> f64 {
        self.origin_ms
    }

    pub fn watermark_ms(&self) -> f64 {
        self.watermark_ms
    }

    pub fn is_overloaded(&self) -> bool {
        self.overloaded
    }

    pub fn revision(&self) -> u64 {
        self.revision
    }

    pub fn observe(
        &mut self,
        hit: ObservedHit,
        calibrated_ms: f64,
    ) -> Result<(), IncrementalScoringError> {
        let arrival = self.next_arrival;
        self.next_arrival += 1;
        self.observe_with_arrival(hit, calibrated_ms, arrival)
    }

    fn observe_with_arrival(
        &mut self,
        hit: ObservedHit,
        calibrated_ms: f64,
        arrival: usize,
    ) -> Result<(), IncrementalScoringError> {
        if !calibrated_ms.is_finite() {
            return Err(IncrementalScoringError::NonFiniteTime);
        }
        if calibrated_ms + 0.000_001 < self.watermark_ms {
            return Err(IncrementalScoringError::LateDelivery {
                event_ms: calibrated_ms,
                watermark_ms: self.watermark_ms,
            });
        }
        self.observed.push(IncrementalObserved {
            hit,
            calibrated_ms,
            arrival,
            consumed: false,
            confirmed_invalid: false,
        });
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    pub fn advance_event_time(&mut self, watermark_ms: f64) -> Result<(), IncrementalScoringError> {
        if !watermark_ms.is_finite() {
            return Err(IncrementalScoringError::NonFiniteTime);
        }
        if watermark_ms <= self.watermark_ms {
            return Ok(());
        }
        self.watermark_ms = watermark_ms;
        let match_ms = self.config.boundaries().2;
        while let Some(&expected_index) = self.expected_order.get(self.next_expected) {
            let decision_frontier = self.expected[expected_index].target_ms + 2.0 * match_ms;
            if decision_frontier + 0.000_001 >= watermark_ms {
                break;
            }
            // Settle at this note's own event-time frontier even when a frame
            // advances across several notes. A delayed frame or final drain
            // must not give an old note access to future-timestamped hits.
            self.finalize_expected(expected_index, decision_frontier + 0.000_001)?;
            self.next_expected += 1;
        }
        self.finalize_invalid_hits(watermark_ms);
        Ok(())
    }

    pub fn retime(&mut self, bpm: f64, tempo_map: &[TempoChange]) {
        self.bpm = bpm;
        self.tempo_map = tempo_map.to_vec();
        for expected in &mut self.expected {
            if !expected.confirmed {
                expected.target_ms = self.origin_ms
                    + elapsed_ms_at_beat(expected.hit.beat, self.bpm, &self.tempo_map);
            }
        }
        self.expected_order.sort_by(|left, right| {
            self.expected[*left]
                .target_ms
                .total_cmp(&self.expected[*right].target_ms)
                .then(left.cmp(right))
        });
        self.next_expected = self
            .expected_order
            .partition_point(|index| self.expected[*index].confirmed);
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn rebase_origin(&mut self, next_origin_ms: f64) {
        let shift = next_origin_ms - self.origin_ms;
        if !shift.is_finite() || shift == 0.0 {
            return;
        }
        self.origin_ms = next_origin_ms;
        if self.watermark_ms.is_finite() {
            self.watermark_ms += shift;
        }
        for expected in &mut self.expected {
            expected.target_ms += shift;
        }
        for observed in &mut self.observed {
            observed.calibrated_ms += shift;
            observed.hit.timestamp_ms += shift;
        }
        for result in self.expected_results.iter_mut().flatten() {
            result.expected_timestamp_ms = result.expected_timestamp_ms.map(|value| value + shift);
            result.observed_ms = result.observed_ms.map(|value| value + shift);
            result.actual_timestamp_ms = result.actual_timestamp_ms.map(|value| value + shift);
        }
        for result in &mut self.invalid_results {
            result.observed_ms = result.observed_ms.map(|value| value + shift);
            result.actual_timestamp_ms = result.actual_timestamp_ms.map(|value| value + shift);
        }
        self.revision = self.revision.wrapping_add(1);
    }

    fn local_plan(
        &mut self,
        expected_indices: &[usize],
        observed_indices: &[usize],
    ) -> Result<LocalPlan, IncrementalScoringError> {
        let work = expected_indices
            .len()
            .saturating_mul(observed_indices.len());
        if work > SCORING_LOCAL_WORK_LIMIT {
            self.overloaded = true;
            return Err(IncrementalScoringError::Overloaded {
                expected: expected_indices.len(),
                observed: observed_indices.len(),
            });
        }
        let rows = expected_indices.len();
        let columns = observed_indices.len();
        let mut plans = vec![vec![LocalPlan::default(); columns + 1]; rows + 1];
        let match_ms = self.config.boundaries().2;
        for row in (0..rows).rev() {
            for column in (0..columns).rev() {
                let mut best = plans[row + 1][column].clone();
                let skip_observed = plans[row][column + 1].clone();
                if prefer_local_plan(&skip_observed, &best) {
                    best = skip_observed;
                }
                let expected_index = expected_indices[row];
                let observed_index = observed_indices[column];
                let expected = &self.expected[expected_index];
                let observed = &self.observed[observed_index];
                let error = observed.calibrated_ms - expected.target_ms;
                if expected.hit.instrument == observed.hit.instrument
                    && error.is_finite()
                    && error.abs() <= match_ms
                {
                    let mut matched = plans[row + 1][column + 1].clone();
                    matched.utility_points += self.config.note_score(error).round() as i32
                        + SCORING_INVALID_HIT_PENALTY.round() as i32;
                    matched.matches += 1;
                    matched.exact_notes +=
                        usize::from(observed.hit.raw_note == Some(expected.hit.note));
                    matched.total_error_ns += (error.abs() * 1_000_000.0).round() as i64;
                    matched.pairs.insert(0, (expected_index, observed_index));
                    if prefer_local_plan(&matched, &best) {
                        best = matched;
                    }
                }
                plans[row][column] = best;
            }
        }
        Ok(plans[0][0].clone())
    }

    fn local_candidates(
        &self,
        instrument: Instrument,
        watermark_ms: f64,
        include_future_observed: bool,
        maximum_expected_ms: Option<f64>,
    ) -> (Vec<usize>, Vec<usize>) {
        let match_ms = self.config.boundaries().2;
        let observed = self
            .observed
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                !item.consumed
                    && !item.confirmed_invalid
                    && item.hit.instrument == instrument
                    && (include_future_observed || item.calibrated_ms <= watermark_ms + 0.000_001)
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let mut expected = self
            .expected
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.confirmed && item.hit.instrument == instrument)
            .filter(|(_, item)| {
                maximum_expected_ms.is_none_or(|maximum| item.target_ms <= maximum + 0.000_001)
            })
            .filter(|(_, item)| {
                observed.iter().any(|observed_index| {
                    (self.observed[*observed_index].calibrated_ms - item.target_ms).abs()
                        <= match_ms
                })
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        expected.sort_by(|left, right| {
            self.expected[*left]
                .target_ms
                .total_cmp(&self.expected[*right].target_ms)
                .then(left.cmp(right))
        });
        let mut observed = observed;
        observed.sort_by(|left, right| {
            self.observed[*left]
                .calibrated_ms
                .total_cmp(&self.observed[*right].calibrated_ms)
                .then(
                    self.observed[*left]
                        .arrival
                        .cmp(&self.observed[*right].arrival),
                )
        });
        (expected, observed)
    }

    fn finalize_expected(
        &mut self,
        expected_index: usize,
        watermark_ms: f64,
    ) -> Result<(), IncrementalScoringError> {
        if self.expected[expected_index].confirmed {
            return Ok(());
        }
        let instrument = self.expected[expected_index].hit.instrument;
        let maximum_expected_ms =
            self.expected[expected_index].target_ms + 2.0 * self.config.boundaries().2;
        let (mut expected_indices, observed_indices) =
            self.local_candidates(instrument, watermark_ms, false, Some(maximum_expected_ms));
        if !expected_indices.contains(&expected_index) {
            expected_indices.push(expected_index);
            expected_indices.sort_by(|left, right| {
                self.expected[*left]
                    .target_ms
                    .total_cmp(&self.expected[*right].target_ms)
                    .then(left.cmp(right))
            });
        }
        let plan = self.local_plan(&expected_indices, &observed_indices)?;
        let matched_observed = plan
            .pairs
            .iter()
            .find_map(|(candidate, observed)| (*candidate == expected_index).then_some(*observed));
        let expected = &self.expected[expected_index];
        let match_ms = self.config.boundaries().2;
        let result = if let Some(observed_index) = matched_observed {
            self.observed[observed_index].consumed = true;
            let observed = &self.observed[observed_index];
            let error = observed.calibrated_ms - expected.target_ms;
            let judgement = self.config.classify(error);
            HitResult {
                expected_id: Some(expected.hit.id.clone()),
                expected_beat: Some(expected.hit.beat),
                expected_timestamp_ms: Some(expected.target_ms),
                observed_ms: Some(observed.hit.timestamp_ms),
                actual_timestamp_ms: Some(observed.hit.timestamp_ms),
                error_ms: Some(error),
                window_ms: match_ms,
                instrument: expected.hit.instrument,
                midi_note: Some(expected.hit.note),
                judgement,
                excellent: judgement == Judgement::Perfect,
                note_score: self.config.note_score(error),
                missed: false,
            }
        } else {
            HitResult {
                expected_id: Some(expected.hit.id.clone()),
                expected_beat: Some(expected.hit.beat),
                expected_timestamp_ms: Some(expected.target_ms),
                observed_ms: None,
                actual_timestamp_ms: None,
                error_ms: None,
                window_ms: match_ms,
                instrument: expected.hit.instrument,
                midi_note: Some(expected.hit.note),
                judgement: Judgement::Missed,
                excellent: false,
                note_score: 0.0,
                missed: true,
            }
        };
        self.expected[expected_index].confirmed = true;
        self.confirmed_log.push(result.clone());
        self.expected_results[expected_index] = Some(result);
        self.revision = self.revision.wrapping_add(1);
        Ok(())
    }

    fn finalize_invalid_hits(&mut self, watermark_ms: f64) {
        let match_ms = self.config.boundaries().2;
        for observed in &mut self.observed {
            if observed.consumed
                || observed.confirmed_invalid
                || observed.calibrated_ms + 3.0 * match_ms + 0.000_001 >= watermark_ms
            {
                continue;
            }
            let wrong_instrument = self.expected.iter().any(|expected| {
                expected.hit.instrument != observed.hit.instrument
                    && (observed.calibrated_ms - expected.target_ms).abs() <= match_ms
            });
            observed.confirmed_invalid = true;
            let result = HitResult {
                expected_id: None,
                expected_beat: None,
                expected_timestamp_ms: None,
                observed_ms: Some(observed.hit.timestamp_ms),
                actual_timestamp_ms: Some(observed.hit.timestamp_ms),
                error_ms: None,
                window_ms: match_ms,
                instrument: observed.hit.instrument,
                midi_note: observed.hit.raw_note,
                judgement: if wrong_instrument {
                    Judgement::WrongInstrument
                } else {
                    Judgement::Extra
                },
                excellent: false,
                note_score: 0.0,
                missed: false,
            };
            self.confirmed_log.push(result.clone());
            self.invalid_results.push(result);
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn provisional_feedback(&mut self) -> Result<Vec<ScoringFeedback>, IncrementalScoringError> {
        let mut feedback = Vec::new();
        let mut projected_observed = vec![false; self.observed.len()];
        let mut instruments = Vec::new();
        for observed in &self.observed {
            if !observed.consumed
                && !observed.confirmed_invalid
                && !instruments.contains(&observed.hit.instrument)
            {
                instruments.push(observed.hit.instrument);
            }
        }
        for instrument in instruments {
            let (expected_indices, observed_indices) =
                self.local_candidates(instrument, self.watermark_ms, true, None);
            let plan = self.local_plan(&expected_indices, &observed_indices)?;
            for (expected_index, observed_index) in plan.pairs {
                projected_observed[observed_index] = true;
                let expected = &self.expected[expected_index];
                let observed = &self.observed[observed_index];
                let error = observed.calibrated_ms - expected.target_ms;
                let judgement = self.config.classify(error);
                feedback.push(ScoringFeedback {
                    result: HitResult {
                        expected_id: Some(expected.hit.id.clone()),
                        expected_beat: Some(expected.hit.beat),
                        expected_timestamp_ms: Some(expected.target_ms),
                        observed_ms: Some(observed.hit.timestamp_ms),
                        actual_timestamp_ms: Some(observed.hit.timestamp_ms),
                        error_ms: Some(error),
                        window_ms: self.config.boundaries().2,
                        instrument: expected.hit.instrument,
                        midi_note: Some(expected.hit.note),
                        judgement,
                        excellent: judgement == Judgement::Perfect,
                        note_score: self.config.note_score(error),
                        missed: false,
                    },
                    confirmed: false,
                });
            }
        }
        let match_ms = self.config.boundaries().2;
        for (index, observed) in self.observed.iter().enumerate() {
            if observed.consumed || observed.confirmed_invalid || projected_observed[index] {
                continue;
            }
            let wrong_instrument = self.expected.iter().any(|expected| {
                !expected.confirmed
                    && expected.hit.instrument != observed.hit.instrument
                    && (observed.calibrated_ms - expected.target_ms).abs() <= match_ms
            });
            feedback.push(ScoringFeedback {
                result: HitResult {
                    expected_id: None,
                    expected_beat: None,
                    expected_timestamp_ms: None,
                    observed_ms: Some(observed.hit.timestamp_ms),
                    actual_timestamp_ms: Some(observed.hit.timestamp_ms),
                    error_ms: None,
                    window_ms: match_ms,
                    instrument: observed.hit.instrument,
                    midi_note: observed.hit.raw_note,
                    judgement: if wrong_instrument {
                        Judgement::WrongInstrument
                    } else {
                        Judgement::Extra
                    },
                    excellent: false,
                    note_score: 0.0,
                    missed: false,
                },
                confirmed: false,
            });
        }
        feedback.sort_by(|left, right| {
            left.result
                .expected_timestamp_ms
                .unwrap_or_else(|| left.result.actual_timestamp_ms.unwrap_or(f64::INFINITY))
                .total_cmp(
                    &right.result.expected_timestamp_ms.unwrap_or_else(|| {
                        right.result.actual_timestamp_ms.unwrap_or(f64::INFINITY)
                    }),
                )
                .then_with(|| left.confirmed.cmp(&right.confirmed).reverse())
        });
        Ok(feedback)
    }

    pub fn feedback(&mut self) -> Result<Vec<ScoringFeedback>, IncrementalScoringError> {
        let mut feedback = self
            .expected_results
            .iter()
            .flatten()
            .cloned()
            .chain(self.invalid_results.iter().cloned())
            .map(|result| ScoringFeedback {
                result,
                confirmed: true,
            })
            .collect::<Vec<_>>();
        feedback.extend(self.provisional_feedback()?);
        Ok(feedback)
    }

    /// Returns newly confirmed results plus the complete current provisional
    /// projection. Confirmed results already returned by this method are not
    /// cloned again, keeping the real-time path independent of attempt length.
    pub fn take_feedback_update(
        &mut self,
    ) -> Result<Vec<ScoringFeedback>, IncrementalScoringError> {
        let mut feedback = self.confirmed_log[self.feedback_cursor..]
            .iter()
            .cloned()
            .map(|result| ScoringFeedback {
                result,
                confirmed: true,
            })
            .collect::<Vec<_>>();
        self.feedback_cursor = self.confirmed_log.len();
        feedback.extend(self.provisional_feedback()?);
        Ok(feedback)
    }

    pub fn finish(&mut self) -> Result<SessionSummary, IncrementalScoringError> {
        let match_ms = self.config.boundaries().2;
        let last_expected = self
            .expected
            .iter()
            .map(|item| item.target_ms)
            .max_by(f64::total_cmp)
            .unwrap_or(self.origin_ms);
        let last_observed = self
            .observed
            .iter()
            .map(|item| item.calibrated_ms)
            .max_by(f64::total_cmp)
            .unwrap_or(self.origin_ms);
        self.advance_event_time(last_expected + 2.0 * match_ms + 0.001)?;
        self.advance_event_time(last_observed + 3.0 * match_ms + 0.001)?;
        Ok(self.summary())
    }

    pub fn summary(&self) -> SessionSummary {
        let results = self
            .expected_results
            .iter()
            .flatten()
            .cloned()
            .chain(self.invalid_results.iter().cloned())
            .collect::<Vec<_>>();
        summary_from_results(self.expected.len(), results)
    }
}

pub fn summary_from_results(expected: usize, results: Vec<HitResult>) -> SessionSummary {
    let matched_results = results
        .iter()
        .filter(|result| result.expected_id.is_some() && result.error_ms.is_some())
        .collect::<Vec<_>>();
    let matched = matched_results.len();
    let missed = expected.saturating_sub(matched);
    let extras = results
        .iter()
        .filter(|result| result.judgement == Judgement::Extra)
        .count();
    let wrong_instrument = results
        .iter()
        .filter(|result| result.judgement == Judgement::WrongInstrument)
        .count();
    let score_sum = matched_results
        .iter()
        .map(|result| result.note_score)
        .sum::<f64>();
    let invalid_penalty = SCORING_INVALID_HIT_PENALTY * (extras + wrong_instrument) as f64;
    let score = (expected > 0).then(|| {
        (((score_sum - invalid_penalty) / expected as f64).clamp(0.0, 100.0) * 10.0).round() / 10.0
    });
    let errors = matched_results
        .iter()
        .filter_map(|result| result.error_ms)
        .collect::<Vec<_>>();
    let mean_abs_error_ms = (!errors.is_empty())
        .then(|| errors.iter().map(|value| value.abs()).sum::<f64>() / errors.len() as f64);
    let signed_mean_error_ms =
        (!errors.is_empty()).then(|| errors.iter().sum::<f64>() / errors.len() as f64);
    let timing_consistency_ms = signed_mean_error_ms.map(|mean| {
        (errors
            .iter()
            .map(|error| (error - mean).powi(2))
            .sum::<f64>()
            / errors.len() as f64)
            .sqrt()
    });
    SessionSummary {
        score,
        matched,
        expected,
        missed,
        extras,
        wrong_instrument,
        mean_abs_error_ms,
        signed_mean_error_ms,
        timing_consistency_ms,
        scoring_version: SCORING_VERSION.into(),
        results,
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PerformanceAward {
    None,
    OneStar,
    TwoStars,
    Completed,
    Mastered,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PerformanceRating {
    pub stars: u8,
    pub award: PerformanceAward,
}

/// Derives the visible achievement tier from a v3 session summary.
///
/// Completion is deliberately attainable without absolute perfection, while
/// gold mastery requires exceptional precision and a clean performance.
pub fn performance_rating(summary: &SessionSummary) -> PerformanceRating {
    let score = summary.score.unwrap_or(0.0);
    let expected = summary.expected;
    let invalid = summary.extras + summary.wrong_instrument;
    let perfect = summary
        .results
        .iter()
        .filter(|result| result.expected_id.is_some())
        .filter(|result| result.judgement == Judgement::Perfect)
        .count();
    let ratio = |count: usize| {
        if expected == 0 {
            0.0
        } else {
            count as f64 / expected as f64
        }
    };

    let short_exercise_mastery = expected < 20 && perfect == expected;
    let long_exercise_mastery = expected >= 20 && ratio(perfect) >= 0.95;
    let mastered = expected > 0
        && score >= 99.0
        && summary.missed == 0
        && invalid == 0
        && (short_exercise_mastery || long_exercise_mastery)
        && summary.mean_abs_error_ms.is_some_and(|value| value <= 25.0)
        && summary
            .timing_consistency_ms
            .is_some_and(|value| value <= 20.0);
    if mastered {
        return PerformanceRating {
            stars: 3,
            award: PerformanceAward::Mastered,
        };
    }

    let completed = expected > 0
        && score >= 95.0
        && ratio(summary.matched) >= 0.98
        && ratio(summary.missed) <= 0.02
        && ratio(invalid) <= 0.01;
    if completed {
        return PerformanceRating {
            stars: 3,
            award: PerformanceAward::Completed,
        };
    }
    if score >= 75.0 {
        PerformanceRating {
            stars: 2,
            award: PerformanceAward::TwoStars,
        }
    } else if score >= 50.0 {
        PerformanceRating {
            stars: 1,
            award: PerformanceAward::OneStar,
        }
    } else {
        PerformanceRating {
            stars: 0,
            award: PerformanceAward::None,
        }
    }
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
        DEFAULT_MATCH_MS.min(0.4 * nearest).max(35.0)
    } else {
        DEFAULT_MATCH_MS
    }
}

/// Scores a complete session by replaying it through the authoritative v4
/// event-time engine. This deliberately is not a separate final matcher.
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
    score_session_with_tempo_map(expected, observed, bpm, start_ms, latency_ms, config, &[])
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
    let mut scorer = IncrementalScorer::new(expected, bpm, start_ms, config, tempo_map);
    let mut order = (0..observed.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| {
        observed[*left]
            .timestamp_ms
            .total_cmp(&observed[*right].timestamp_ms)
            .then(left.cmp(right))
    });
    for observed_index in order {
        let hit = observed[observed_index].clone();
        let calibrated_ms = hit.timestamp_ms - latency_ms;
        // A finite, complete offline session cannot violate the delivery
        // watermark. Non-finite events remain observable as invalid hits by
        // excluding them from matching rather than panicking.
        if calibrated_ms.is_finite() {
            scorer
                .observe_with_arrival(hit, calibrated_ms, observed_index)
                .expect("offline scorer has not advanced its event-time frontier");
        }
    }
    scorer
        .finish()
        .expect("offline scoring exceeded the explicit v4 local-work limit")
}

// Retained only so the checked-in feasibility studies can compare their
// historical v3 baselines after production moves to v4. Application code must
// use IncrementalScorer or the unversioned score_session wrappers above.
#[doc(hidden)]
pub fn score_session_v3_with_tempo_map(
    expected: &[ExpectedHit],
    observed: &[ObservedHit],
    bpm: f64,
    start_ms: f64,
    latency_ms: f64,
    config: ScoringConfig,
    tempo_map: &[TempoChange],
) -> SessionSummary {
    #[derive(Clone, Copy, Default)]
    enum Action {
        #[default]
        Done,
        SkipExpected,
        SkipObserved,
        Match,
    }
    #[derive(Clone, Copy, Default)]
    struct Plan {
        matched: usize,
        exact: usize,
        error: f64,
        action: Action,
    }
    fn preferred(candidate: Plan, current: Plan) -> bool {
        candidate.matched > current.matched
            || (candidate.matched == current.matched
                && (candidate.exact > current.exact
                    || (candidate.exact == current.exact
                        && (candidate.error < current.error
                            || (candidate.error == current.error
                                && matches!(candidate.action, Action::Match)
                                && !matches!(current.action, Action::Match))))))
    }

    let match_ms = config.boundaries().2;
    let mut used = vec![false; observed.len()];
    let mut matches = vec![None; expected.len()];
    let mut instruments = Vec::new();
    for hit in expected {
        if !instruments.contains(&hit.instrument) {
            instruments.push(hit.instrument);
        }
    }
    for instrument in instruments {
        let mut expected_order = expected
            .iter()
            .enumerate()
            .filter_map(|(index, hit)| (hit.instrument == instrument).then_some(index))
            .collect::<Vec<_>>();
        expected_order.sort_by(|left, right| {
            expected[*left]
                .beat
                .total_cmp(&expected[*right].beat)
                .then(left.cmp(right))
        });
        let mut observed_order = observed
            .iter()
            .enumerate()
            .filter_map(|(index, hit)| (hit.instrument == instrument).then_some(index))
            .collect::<Vec<_>>();
        observed_order.sort_by(|left, right| {
            observed[*left]
                .timestamp_ms
                .total_cmp(&observed[*right].timestamp_ms)
                .then(left.cmp(right))
        });
        let rows = expected_order.len();
        let columns = observed_order.len();
        let mut plans = vec![vec![Plan::default(); columns + 1]; rows + 1];
        for row in (0..rows).rev() {
            for column in (0..columns).rev() {
                let mut best = Plan {
                    action: Action::SkipExpected,
                    ..plans[row + 1][column]
                };
                let skip = Plan {
                    action: Action::SkipObserved,
                    ..plans[row][column + 1]
                };
                if preferred(skip, best) {
                    best = skip;
                }
                let expected_index = expected_order[row];
                let observed_index = observed_order[column];
                let target =
                    start_ms + elapsed_ms_at_beat(expected[expected_index].beat, bpm, tempo_map);
                let error = observed[observed_index].timestamp_ms - latency_ms - target;
                if error.is_finite() && error.abs() <= match_ms {
                    let child = plans[row + 1][column + 1];
                    let matched = Plan {
                        matched: child.matched + 1,
                        exact: child.exact
                            + usize::from(
                                observed[observed_index].raw_note
                                    == Some(expected[expected_index].note),
                            ),
                        error: child.error + error.abs(),
                        action: Action::Match,
                    };
                    if preferred(matched, best) {
                        best = matched;
                    }
                }
                plans[row][column] = best;
            }
        }
        let (mut row, mut column) = (0, 0);
        while row < rows && column < columns {
            match plans[row][column].action {
                Action::SkipExpected => row += 1,
                Action::SkipObserved => column += 1,
                Action::Match => {
                    let expected_index = expected_order[row];
                    let observed_index = observed_order[column];
                    let target = start_ms
                        + elapsed_ms_at_beat(expected[expected_index].beat, bpm, tempo_map);
                    matches[expected_index] = Some((
                        observed_index,
                        observed[observed_index].timestamp_ms - latency_ms - target,
                    ));
                    used[observed_index] = true;
                    row += 1;
                    column += 1;
                }
                Action::Done => break,
            }
        }
    }

    let mut results = Vec::with_capacity(expected.len() + observed.len());
    for (index, hit) in expected.iter().enumerate() {
        let target = start_ms + elapsed_ms_at_beat(hit.beat, bpm, tempo_map);
        if let Some((observed_index, error)) = matches[index] {
            let judgement = config.classify(error);
            results.push(HitResult {
                expected_id: Some(hit.id.clone()),
                expected_beat: Some(hit.beat),
                expected_timestamp_ms: Some(target),
                observed_ms: Some(observed[observed_index].timestamp_ms),
                actual_timestamp_ms: Some(observed[observed_index].timestamp_ms),
                error_ms: Some(error),
                window_ms: match_ms,
                instrument: hit.instrument,
                midi_note: Some(hit.note),
                judgement,
                excellent: judgement == Judgement::Perfect,
                note_score: config.note_score(error),
                missed: false,
            });
        } else {
            results.push(HitResult {
                expected_id: Some(hit.id.clone()),
                expected_beat: Some(hit.beat),
                expected_timestamp_ms: Some(target),
                observed_ms: None,
                actual_timestamp_ms: None,
                error_ms: None,
                window_ms: match_ms,
                instrument: hit.instrument,
                midi_note: Some(hit.note),
                judgement: Judgement::Missed,
                excellent: false,
                note_score: 0.0,
                missed: true,
            });
        }
    }
    for (index, hit) in observed.iter().enumerate() {
        if used[index] {
            continue;
        }
        let wrong = expected.iter().any(|candidate| {
            candidate.instrument != hit.instrument
                && (hit.timestamp_ms
                    - latency_ms
                    - (start_ms + elapsed_ms_at_beat(candidate.beat, bpm, tempo_map)))
                .abs()
                    <= match_ms
        });
        results.push(HitResult {
            expected_id: None,
            expected_beat: None,
            expected_timestamp_ms: None,
            observed_ms: Some(hit.timestamp_ms),
            actual_timestamp_ms: Some(hit.timestamp_ms),
            error_ms: None,
            window_ms: 0.0,
            instrument: hit.instrument,
            midi_note: hit.raw_note,
            judgement: if wrong {
                Judgement::WrongInstrument
            } else {
                Judgement::Extra
            },
            excellent: false,
            note_score: 0.0,
            missed: false,
        });
    }
    let mut summary = summary_from_results(expected.len(), results);
    summary.scoring_version = "v3".into();
    summary
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
        assert_eq!(a.score, Some(100.0));
        assert_eq!(a, b);
        assert_eq!(a.scoring_version, SCORING_VERSION);
    }
    #[test]
    fn early_late_and_missing_extra_classify() {
        let e = paradiddle();
        let mut o = vec![obs(-61.0, Instrument::Snare), obs(561.0, Instrument::Snare)];
        o.push(obs(9_999.0, Instrument::Snare));
        let s = score_session(&e.hits[..3], &o, 60.0, 0.0, 0.0);
        assert_eq!(s.results[0].judgement, Judgement::Early);
        assert_eq!(s.results[1].judgement, Judgement::Late);
        assert_eq!(s.missed, 1);
        assert_eq!(s.extras, 1);
        assert!(s.score.unwrap() < 100.0);
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
    fn imported_midi_preserves_sha256_hash_and_id() {
        let bytes = include_bytes!("../../../web/public/exercises/rudiments/01_roll.mid");
        let exercise = parse_midi(bytes, "reference.mid", None).unwrap();
        let renamed = parse_midi(bytes, "renamed.mid", Some(exercise.selected_track)).unwrap();

        assert_eq!(
            exercise.content_hash,
            "eed470e0d21d9d73bd083dd30d6178aa9a5bb1a6bc0f70d3585e71ceaa5ba417"
        );
        assert_eq!(exercise.id, "import-eed470e0d21d");
        assert_eq!(renamed.content_hash, exercise.content_hash);
        assert_eq!(renamed.id, exercise.id);
    }
    #[test]
    fn corrupt_midi_is_rejected() {
        assert!(parse_midi(b"MThd\0", "bad.mid", None).is_err());
    }
    fn minimal_midi(
        timing: Timing,
        metadata: Option<MetaMessage<'static>>,
        include_note: bool,
    ) -> Vec<u8> {
        use midly::{
            Header, TrackEvent,
            num::{u4, u7, u28},
        };
        let mut track = Vec::new();
        if let Some(message) = metadata {
            track.push(TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(message),
            });
        }
        if include_note {
            track.extend([
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
                    delta: u28::new(120),
                    kind: TrackEventKind::Midi {
                        channel: u4::new(9),
                        message: MidiMessage::NoteOff {
                            key: u7::new(38),
                            vel: u7::new(0),
                        },
                    },
                },
            ]);
        }
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        let smf = Smf {
            header: Header::new(Format::SingleTrack, timing),
            tracks: vec![track],
        };
        let mut bytes = Vec::new();
        smf.write_std(&mut bytes).unwrap();
        bytes
    }
    #[test]
    fn parser_rejects_unsafe_sizes_and_invalid_timing_metadata() {
        use midly::num::{u15, u24};

        assert!(matches!(
            parse_midi(&vec![0; MAX_MIDI_FILE_BYTES], "limit.mid", None),
            Err(MidiError::Parse(_))
        ));
        assert!(matches!(
            parse_midi(&vec![0; MAX_MIDI_FILE_BYTES + 1], "large.mid", None),
            Err(MidiError::FileTooLarge)
        ));
        assert!(matches!(
            parse_midi(
                &minimal_midi(Timing::Metrical(u15::new(0)), None, true),
                "zero-ppq.mid",
                None
            ),
            Err(MidiError::InvalidPpq)
        ));
        assert!(matches!(
            parse_midi(
                &minimal_midi(
                    Timing::Metrical(u15::new(480)),
                    Some(MetaMessage::Tempo(u24::new(0))),
                    true
                ),
                "zero-tempo.mid",
                None
            ),
            Err(MidiError::InvalidTempo)
        ));
        assert!(matches!(
            parse_midi(
                &minimal_midi(
                    Timing::Metrical(u15::new(480)),
                    Some(MetaMessage::TimeSignature(4, 8, 24, 8)),
                    true
                ),
                "invalid-meter.mid",
                None
            ),
            Err(MidiError::InvalidMeter)
        ));
        assert!(matches!(
            parse_midi(
                &minimal_midi(Timing::Metrical(u15::new(480)), None, false),
                "empty.mid",
                None
            ),
            Err(MidiError::NoNotes)
        ));
    }
    #[test]
    fn parser_rejects_pathological_event_counts_before_processing_notes() {
        use midly::{
            Header, TrackEvent,
            num::{u4, u7, u15, u28},
        };

        let mut track = Vec::with_capacity(100_001);
        for _ in 0..100_000 {
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
        }
        track.push(TrackEvent {
            delta: u28::new(0),
            kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
        });
        let smf = Smf {
            header: Header::new(Format::SingleTrack, Timing::Metrical(u15::new(480))),
            tracks: vec![track],
        };
        let mut bytes = Vec::new();
        smf.write_std(&mut bytes).unwrap();

        assert!(matches!(
            parse_midi(&bytes, "event-bomb.mid", None),
            Err(MidiError::TooManyEvents)
        ));
    }
    #[test]
    fn ambiguous_lyrics_are_not_guessed() {
        let e = parse_midi(&fixture(true), "ambiguous.mid", None).unwrap();
        assert!(e.hits.iter().all(|h| h.hand == Hand::Unknown));
        assert!(e.warnings.iter().any(|w| w.contains("ambiguous")));
    }
    #[test]
    fn bundled_paradiddle_model_has_expected_grid_and_sticking() {
        let e = paradiddle();
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
    fn public_domain_rudiment_library_matches_reference_manifest() {
        type ReferenceFixture = (
            &'static str,
            &'static [u8],
            usize,
            usize,
            usize,
            usize,
            usize,
        );
        let fixtures: [ReferenceFixture; 11] = [
            (
                "Roll.mid",
                include_bytes!("../../../web/public/exercises/rudiments/01_roll.mid"),
                64,
                4,
                0,
                8,
                0,
            ),
            (
                "Mother or 5 S. Roll.mid",
                include_bytes!("../../../web/public/exercises/rudiments/02_mother_or_5_s_roll.mid"),
                40,
                4,
                0,
                8,
                0,
            ),
            (
                "7 Stroke Roll.mid",
                include_bytes!("../../../web/public/exercises/rudiments/03_7_stroke_roll.mid"),
                56,
                4,
                0,
                8,
                0,
            ),
            (
                "9 Stroke Roll.mid",
                include_bytes!("../../../web/public/exercises/rudiments/04_9_stroke_roll.mid"),
                36,
                4,
                0,
                4,
                0,
            ),
            (
                "Hard Flams.mid",
                include_bytes!("../../../web/public/exercises/rudiments/05_hard_flams.mid"),
                64,
                4,
                32,
                32,
                0,
            ),
            (
                "Single Paradiddle.mid",
                include_bytes!("../../../web/public/exercises/rudiments/06_single_paradiddle.mid"),
                64,
                4,
                0,
                16,
                0,
            ),
            (
                "Double Paradiddle.mid",
                include_bytes!("../../../web/public/exercises/rudiments/07_double_paradiddle.mid"),
                48,
                4,
                0,
                8,
                48,
            ),
            (
                "Flam Paradiddle.mid",
                include_bytes!("../../../web/public/exercises/rudiments/08_flam_paradiddle.mid"),
                80,
                4,
                16,
                16,
                0,
            ),
            (
                "Single Drag.mid",
                include_bytes!("../../../web/public/exercises/rudiments/09_single_drag.mid"),
                96,
                4,
                64,
                16,
                0,
            ),
            (
                "Single Rotamacue.mid",
                include_bytes!("../../../web/public/exercises/rudiments/10_single_rotamacue.mid"),
                80,
                4,
                32,
                16,
                48,
            ),
            (
                "The Drummers Call — Advanced Study.mid",
                include_bytes!(
                    "../../../web/public/exercises/rudiments/11_advanced_the_drummers_call_study.mid"
                ),
                122,
                8,
                32,
                29,
                24,
            ),
        ];

        for (name, bytes, note_count, bars, grace_count, accent_count, tuplet_count) in fixtures {
            let exercise = parse_midi(bytes, name, None).unwrap();
            assert_eq!(exercise.hits.len(), note_count, "{name}");
            assert_eq!(
                exercise.hits.iter().filter(|hit| hit.grace).count(),
                grace_count,
                "{name}"
            );
            assert_eq!(
                exercise.hits.iter().filter(|hit| hit.accent).count(),
                accent_count,
                "{name}"
            );
            assert_eq!(
                exercise
                    .hits
                    .iter()
                    .filter(
                        |hit| !hit.grace && (hit.notation_duration_beats - 1.0 / 3.0).abs() < 0.01
                    )
                    .count(),
                tuplet_count,
                "{name}"
            );
            assert!(
                exercise.hits.iter().all(|hit| hit.hand != Hand::Unknown),
                "{name}"
            );
            assert!(exercise.hits.iter().all(|hit| hit.note == 38), "{name}");
            assert!((exercise.tempo_bpm - 72.0).abs() < 0.01, "{name}");
            assert!(
                exercise.hits.last().unwrap().beat >= (bars - 1) as f64 * 4.0,
                "{name}"
            );
            assert!(
                exercise.hits.last().unwrap().beat < bars as f64 * 4.0,
                "{name}"
            );
            assert!(
                exercise.warnings.is_empty(),
                "{name}: {:?}",
                exercise.warnings
            );
        }
    }
    #[test]
    fn smf1_multitrack_uses_absolute_deltas_and_metadata() {
        let bytes = parallel_fixture((3, 2));
        let e = parse_midi(&bytes, "parallel.mid", None).unwrap();
        assert_eq!(e.track_names, vec!["Conductor", "Drums", "Other"]);
        assert_eq!(e.playable_tracks.len(), 1);
        assert_eq!(e.playable_tracks[0].index, 1);
        assert_eq!(
            e.playable_tracks[0].instruments,
            vec![Instrument::Snare, Instrument::Kick, Instrument::PedalHat]
        );
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
        assert_eq!(
            e.hits.iter().map(|hit| hit.hand).collect::<Vec<_>>(),
            vec![Hand::R, Hand::L, Hand::R]
        );
        assert!(
            !e.warnings
                .iter()
                .any(|warning| warning.contains("no aligned R/L"))
        );
        assert!(
            !e.warnings
                .iter()
                .any(|warning| warning.contains("Multiple note tracks"))
        );
        let other = parse_midi(&bytes, "parallel.mid", Some(2)).unwrap();
        assert_eq!(other.selected_track, 2);
    }
    #[test]
    fn format_zero_with_channel_ten_ignores_melodic_channels() {
        let exercise = parse_midi(&mixed_format_zero_fixture(), "mixed.mid", None).unwrap();
        assert_eq!(exercise.hits.len(), 2);
        assert!(exercise.hits.iter().all(|hit| hit.channel == 9));
        assert_eq!(
            exercise
                .hits
                .iter()
                .map(|hit| hit.instrument)
                .collect::<Vec<_>>(),
            vec![Instrument::Snare, Instrument::Kick]
        );
        assert_eq!(exercise.playable_tracks.len(), 1);
        assert!(
            !exercise.playable_tracks[0]
                .instruments
                .contains(&Instrument::OtherPercussion)
        );
    }
    #[test]
    fn partial_local_sticking_is_completed_from_annotation_tracks() {
        let exercise = parse_midi(&partial_sticking_fixture(), "partial.mid", None).unwrap();
        assert_eq!(
            exercise.hits.iter().map(|hit| hit.hand).collect::<Vec<_>>(),
            vec![Hand::R, Hand::L]
        );
        assert!(
            !exercise
                .warnings
                .iter()
                .any(|warning| warning.contains("no aligned R/L"))
        );
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
            &[obs(target + 89.0, Instrument::Snare)],
            60.0,
            0.0,
            0.0,
        );
        assert_eq!(s.matched, 1);
        assert_eq!(s.missed, 0);
        assert_eq!(s.results[0].judgement, Judgement::Late);
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
                grace: false,
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
            assert!(
                hits.iter()
                    .all(|hit| { (hit.notation_duration_beats - expected_duration).abs() < 0.001 }),
                "expected duration {expected_duration}, got {:?}",
                hits.iter()
                    .map(|hit| hit.notation_duration_beats)
                    .collect::<Vec<_>>()
            );
            assert!(
                hits.iter()
                    .zip(beats)
                    .all(|(hit, beat)| (hit.notation_beat - beat).abs() < 0.001)
            );
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
            perfect_ms: 12.0,
            good_ms: 35.0,
            ..ScoringConfig::default()
        };
        let a = score_session_with_config(&simultaneous, &observed, 60.0, 0.0, 0.0, config);
        let b = score_session_with_config(&simultaneous, &observed, 60.0, 0.0, 0.0, config);
        assert_eq!(a, b);
        assert_eq!(a.matched, 2);
        assert!(a.results[0].excellent);
        assert!(!a.results[1].excellent);
        assert_eq!(a.results[1].judgement, Judgement::Good);
    }

    fn expected_sequence(count: usize, spacing_beats: f64) -> Vec<ExpectedHit> {
        let template = paradiddle().hits[0].clone();
        (0..count)
            .map(|index| ExpectedHit {
                id: format!("expected-{index}"),
                tick: (index as f64 * spacing_beats * 480.0).round() as u64,
                beat: index as f64 * spacing_beats,
                notation_beat: index as f64 * spacing_beats,
                ..template.clone()
            })
            .collect()
    }

    #[test]
    fn timing_boundaries_are_exact_and_have_no_gaps() {
        let config = ScoringConfig::default();
        for error in [-20.0, 0.0, 20.0] {
            assert_eq!(config.classify(error), Judgement::Perfect);
        }
        for error in [-60.0, -20.000_001, 20.000_001, 60.0] {
            assert_eq!(config.classify(error), Judgement::Good);
        }
        assert_eq!(config.classify(-60.000_001), Judgement::Early);
        assert_eq!(config.classify(-100.0), Judgement::Early);
        assert_eq!(config.classify(60.000_001), Judgement::Late);
        assert_eq!(config.classify(100.0), Judgement::Late);
        assert_eq!(config.classify(-100.000_001), Judgement::Missed);
        assert_eq!(config.classify(100.000_001), Judgement::Missed);
    }

    #[test]
    fn performance_timestamps_keep_subdivision_independent_precision_for_demo_and_midi() {
        let expected = expected_sequence(7, 1.0);
        let offsets = [0.0, 7.0, 20.0, 43.0, 80.0, -35.0, -101.0];
        let judgements = [
            Judgement::Perfect,
            Judgement::Perfect,
            Judgement::Perfect,
            Judgement::Good,
            Judgement::Late,
            Judgement::Good,
        ];
        let start_ms = 10_000.0;

        for source in [InputSource::Demo, InputSource::Midi] {
            let observed = expected
                .iter()
                .zip(offsets)
                .map(|(hit, offset)| ObservedHit {
                    timestamp_ms: start_ms + hit.beat * 1000.0 + offset,
                    instrument: hit.instrument,
                    velocity: 100,
                    source,
                    raw_note: (source == InputSource::Midi).then_some(38),
                    raw_channel: (source == InputSource::Midi).then_some(10),
                })
                .collect::<Vec<_>>();
            let summary = score_session(&expected, &observed, 60.0, start_ms, 0.0);

            assert_eq!(summary.matched, 6);
            assert_eq!(summary.missed, 1);
            assert_eq!(summary.extras, 1);
            for index in 0..6 {
                let result = &summary.results[index];
                assert_eq!(result.observed_ms, Some(observed[index].timestamp_ms));
                assert_eq!(
                    result.actual_timestamp_ms,
                    Some(observed[index].timestamp_ms)
                );
                assert_eq!(result.error_ms, Some(offsets[index]));
                assert_eq!(result.judgement, judgements[index]);
            }
            assert_eq!(summary.results[6].judgement, Judgement::Missed);
            let unmatched = summary.results.last().unwrap();
            assert_eq!(unmatched.judgement, Judgement::Extra);
            assert_eq!(unmatched.observed_ms, Some(observed[6].timestamp_ms));
            assert_eq!(
                unmatched.actual_timestamp_ms,
                Some(observed[6].timestamp_ms)
            );
        }
    }

    #[test]
    fn irregular_hits_and_bar_boundaries_retain_fractional_ms_at_different_tempos() {
        let template = paradiddle().hits[0].clone();
        for (bpm, beats, offsets) in [
            (60.0, [1.0, 1.137], [7.125, -35.375]),
            (120.0, [3.75, 4.25], [7.25, -35.5]),
        ] {
            let expected = beats
                .into_iter()
                .enumerate()
                .map(|(index, beat)| ExpectedHit {
                    id: format!("precision-{bpm}-{index}"),
                    beat,
                    notation_beat: beat,
                    ..template.clone()
                })
                .collect::<Vec<_>>();
            let observed = expected
                .iter()
                .zip(offsets)
                .map(|(hit, offset)| ObservedHit {
                    timestamp_ms: elapsed_ms_at_beat(hit.beat, bpm, &[]) + offset,
                    instrument: hit.instrument,
                    velocity: 100,
                    source: InputSource::Midi,
                    raw_note: Some(38),
                    raw_channel: Some(10),
                })
                .collect::<Vec<_>>();
            let summary = score_session(&expected, &observed, bpm, 0.0, 0.0);
            assert_eq!(summary.matched, 2);
            for index in 0..2 {
                assert_eq!(
                    summary.results[index].observed_ms,
                    Some(observed[index].timestamp_ms)
                );
                assert!(
                    (summary.results[index].error_ms.unwrap() - offsets[index]).abs() < 0.000_001
                );
            }
        }
    }

    #[test]
    fn note_scores_follow_the_documented_judgement_weights() {
        let config = ScoringConfig::default();
        for (error, expected_score) in [
            (0.0, 100.0),
            (20.0, 100.0),
            (20.000_001, 95.0),
            (60.0, 95.0),
            (60.000_001, 60.0),
            (100.0, 60.0),
            (100.000_001, 0.0),
        ] {
            assert!((config.note_score(error) - expected_score).abs() < 0.000_001);
            assert!((config.note_score(-error) - expected_score).abs() < 0.000_001);
        }
    }

    #[test]
    fn acceptance_perfect_and_single_good_scores() {
        let expected = expected_sequence(100, 0.25);
        let perfect_observed = expected
            .iter()
            .enumerate()
            .map(|(index, hit)| {
                obs(
                    hit.beat * 1000.0 + if index % 2 == 0 { -15.0 } else { 15.0 },
                    hit.instrument,
                )
            })
            .collect::<Vec<_>>();
        let perfect = score_session(&expected, &perfect_observed, 60.0, 0.0, 0.0);
        assert_eq!(perfect.score, Some(100.0));
        assert_eq!(
            perfect
                .results
                .iter()
                .filter(|result| result.judgement == Judgement::Perfect)
                .count(),
            100
        );

        let mut mostly_perfect = perfect_observed;
        mostly_perfect[99].timestamp_ms = expected[99].beat * 1000.0 + 22.5;
        let summary = score_session(&expected, &mostly_perfect, 60.0, 0.0, 0.0);
        assert_eq!(summary.score, Some(100.0));
        assert_eq!(
            summary
                .results
                .iter()
                .filter(|result| result.judgement == Judgement::Good)
                .count(),
            1
        );
    }

    #[test]
    fn documented_example_scores_86_and_awards_two_stars() {
        let expected = expected_sequence(4, 0.5);
        let observed = expected
            .iter()
            .enumerate()
            .map(|(index, hit)| {
                let offset = if index == 3 { -61.0 } else { 30.0 };
                obs(hit.beat * 1000.0 + offset, hit.instrument)
            })
            .collect::<Vec<_>>();
        let summary = score_session(&expected, &observed, 60.0, 0.0, 0.0);
        assert_eq!(summary.score, Some(86.3));
        assert_eq!(
            performance_rating(&summary),
            PerformanceRating {
                stars: 2,
                award: PerformanceAward::TwoStars,
            }
        );
    }

    #[test]
    fn completion_and_gold_mastery_remain_distinct() {
        let expected = expected_sequence(4, 0.5);
        let all_good = expected
            .iter()
            .map(|hit| obs(hit.beat * 1000.0 + 30.0, hit.instrument))
            .collect::<Vec<_>>();
        let completed = score_session(&expected, &all_good, 60.0, 0.0, 0.0);
        assert_eq!(completed.score, Some(95.0));
        assert_eq!(
            performance_rating(&completed).award,
            PerformanceAward::Completed
        );

        let all_perfect = expected
            .iter()
            .map(|hit| obs(hit.beat * 1000.0, hit.instrument))
            .collect::<Vec<_>>();
        let mastered = score_session(&expected, &all_perfect, 60.0, 0.0, 0.0);
        assert_eq!(
            performance_rating(&mastered).award,
            PerformanceAward::Mastered
        );
    }

    #[test]
    fn acceptance_late_missed_and_extra_scenarios() {
        let expected = expected_sequence(100, 0.5);
        let late_observed = expected
            .iter()
            .map(|hit| obs(hit.beat * 1000.0 + 61.0, hit.instrument))
            .collect::<Vec<_>>();
        let late = score_session(&expected, &late_observed, 60.0, 0.0, 0.0);
        assert_eq!(late.score, Some(60.0));
        assert_eq!(late.signed_mean_error_ms, Some(61.0));
        assert_eq!(late.timing_consistency_ms, Some(0.0));
        assert!(
            late.results
                .iter()
                .all(|result| result.judgement == Judgement::Late)
        );

        let first_ninety = expected
            .iter()
            .take(90)
            .map(|hit| obs(hit.beat * 1000.0, hit.instrument))
            .collect::<Vec<_>>();
        let missed = score_session(&expected, &first_ninety, 60.0, 0.0, 0.0);
        assert_eq!(missed.missed, 10);
        assert_eq!(missed.score, Some(90.0));

        let mut with_extras = expected
            .iter()
            .map(|hit| obs(hit.beat * 1000.0, hit.instrument))
            .collect::<Vec<_>>();
        with_extras.extend((0..5).map(|index| obs(100_000.0 + index as f64, Instrument::Snare)));
        let extra = score_session(&expected, &with_extras, 60.0, 0.0, 0.0);
        assert_eq!(extra.extras, 5);
        assert_eq!(extra.score, Some(97.5));
    }

    #[test]
    fn optimal_matching_handles_rapid_repeats_duplicates_and_simultaneous_drums() {
        let rapid = expected_sequence(2, 0.09);
        let rapid_observed = [obs(80.0, Instrument::Snare), obs(170.0, Instrument::Snare)];
        let rapid_summary = score_session(&rapid, &rapid_observed, 60.0, 0.0, 0.0);
        assert_eq!(rapid_summary.matched, 2);
        assert_eq!(rapid_summary.missed, 0);

        let duplicate = score_session(
            &rapid[..1],
            &[obs(0.0, Instrument::Snare), obs(1.0, Instrument::Snare)],
            60.0,
            0.0,
            0.0,
        );
        assert_eq!(duplicate.matched, 1);
        assert_eq!(duplicate.extras, 1);
        assert_eq!(duplicate.score, Some(50.0));

        let mut simultaneous = vec![rapid[0].clone(), rapid[0].clone()];
        simultaneous[1].id = "kick".into();
        simultaneous[1].instrument = Instrument::Kick;
        simultaneous[1].note = 36;
        let simultaneous_summary = score_session(
            &simultaneous,
            &[obs(5.0, Instrument::Kick), obs(-5.0, Instrument::Snare)],
            60.0,
            0.0,
            0.0,
        );
        assert_eq!(simultaneous_summary.matched, 2);
        assert_eq!(simultaneous_summary.score, Some(100.0));
    }

    #[test]
    fn offsets_statistics_empty_sessions_and_rounding_are_accurate() {
        let expected = expected_sequence(2, 1.0);
        let calibrated = score_session(
            &expected[..1],
            &[obs(50.0, Instrument::Snare)],
            60.0,
            0.0,
            50.0,
        );
        assert_eq!(calibrated.results[0].error_ms, Some(0.0));
        assert_eq!(calibrated.score, Some(100.0));

        let statistics = score_session(
            &expected,
            &[
                obs(-10.0, Instrument::Snare),
                obs(1020.0, Instrument::Snare),
            ],
            60.0,
            0.0,
            0.0,
        );
        assert_eq!(statistics.signed_mean_error_ms, Some(5.0));
        assert_eq!(statistics.mean_abs_error_ms, Some(15.0));
        assert_eq!(statistics.timing_consistency_ms, Some(15.0));

        let empty = score_session(&[], &[obs(0.0, Instrument::Snare)], 60.0, 0.0, 0.0);
        assert_eq!(empty.score, None);
        assert_eq!(empty.extras, 1);
        assert_eq!(empty.timing_consistency_ms, None);

        let hundred = expected_sequence(100, 0.5);
        let almost_perfect = hundred
            .iter()
            .enumerate()
            .map(|(index, hit)| {
                obs(
                    hit.beat * 1000.0 + if index == 0 { 20.000_001 } else { 0.0 },
                    hit.instrument,
                )
            })
            .collect::<Vec<_>>();
        let rounded = score_session(&hundred, &almost_perfect, 60.0, 0.0, 0.0);
        assert_eq!(rounded.score, Some(100.0));
        assert_ne!(rounded.results[0].judgement, Judgement::Perfect);
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
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::Text(b"R")),
            },
            TrackEvent {
                delta: u28::new(240),
                kind: TrackEventKind::Meta(MetaMessage::Marker(b"L")),
            },
            TrackEvent {
                delta: u28::new(240),
                kind: TrackEventKind::Meta(MetaMessage::Tempo(u24::new(400_000))),
            },
            TrackEvent {
                delta: u28::new(240),
                kind: TrackEventKind::Meta(MetaMessage::CuePoint(b"R")),
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
    fn mixed_format_zero_fixture() -> Vec<u8> {
        use midly::{
            Format, Header, Smf, TrackEvent, TrackEventKind,
            num::{u4, u7, u15, u28},
        };
        let track = vec![
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Midi {
                    channel: u4::new(0),
                    message: MidiMessage::NoteOn {
                        key: u7::new(60),
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
                        vel: u7::new(100),
                    },
                },
            },
            TrackEvent {
                delta: u28::new(480),
                kind: TrackEventKind::Midi {
                    channel: u4::new(0),
                    message: MidiMessage::NoteOn {
                        key: u7::new(64),
                        vel: u7::new(100),
                    },
                },
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
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
            },
        ];
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
    fn partial_sticking_fixture() -> Vec<u8> {
        use midly::{
            Format, Header, Smf, TrackEvent, TrackEventKind,
            num::{u4, u7, u15, u28},
        };
        let annotations = vec![
            TrackEvent {
                delta: u28::new(480),
                kind: TrackEventKind::Meta(MetaMessage::Lyric(b"L")),
            },
            TrackEvent {
                delta: u28::new(0),
                kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
            },
        ];
        let drums = vec![
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
            TrackEvent {
                delta: u28::new(480),
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
                kind: TrackEventKind::Meta(MetaMessage::EndOfTrack),
            },
        ];
        let smf = Smf {
            header: Header {
                format: Format::Parallel,
                timing: Timing::Metrical(u15::new(480)),
            },
            tracks: vec![annotations, drums],
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

    fn dense_expected(count: usize, spacing_ms: f64) -> Vec<ExpectedHit> {
        let template = paradiddle().hits[0].clone();
        (0..count)
            .map(|index| ExpectedHit {
                id: format!("dense-{index}"),
                beat: index as f64 * spacing_ms / 1000.0,
                notation_beat: index as f64 * spacing_ms / 1000.0,
                tick: index as u64,
                ..template.clone()
            })
            .collect()
    }

    #[derive(Clone, Default)]
    struct ReferencePlan {
        utility: i32,
        matches: usize,
        exact: usize,
        error_ns: i64,
        pairs: Vec<(usize, usize)>,
    }

    fn reference_preferred(candidate: &ReferencePlan, current: &ReferencePlan) -> bool {
        candidate.utility > current.utility
            || (candidate.utility == current.utility
                && (candidate.matches > current.matches
                    || (candidate.matches == current.matches
                        && (candidate.exact > current.exact
                            || (candidate.exact == current.exact
                                && (candidate.error_ns < current.error_ns
                                    || (candidate.error_ns == current.error_ns
                                        && candidate.pairs < current.pairs)))))))
    }

    fn reference_local_plan(
        expected: &[ExpectedHit],
        targets: &[f64],
        observed: &[ObservedHit],
        expected_indices: &[usize],
        observed_indices: &[usize],
        config: ScoringConfig,
    ) -> ReferencePlan {
        #[allow(clippy::too_many_arguments)]
        fn visit(
            expected: &[ExpectedHit],
            targets: &[f64],
            observed: &[ObservedHit],
            expected_indices: &[usize],
            observed_indices: &[usize],
            config: ScoringConfig,
            expected_cursor: usize,
            observed_cursor: usize,
            plan: &mut ReferencePlan,
            best: &mut ReferencePlan,
        ) {
            if expected_cursor == expected_indices.len() {
                if reference_preferred(plan, best) {
                    *best = plan.clone();
                }
                return;
            }
            visit(
                expected,
                targets,
                observed,
                expected_indices,
                observed_indices,
                config,
                expected_cursor + 1,
                observed_cursor,
                plan,
                best,
            );
            let expected_index = expected_indices[expected_cursor];
            for candidate in observed_cursor..observed_indices.len() {
                let observed_index = observed_indices[candidate];
                let error = observed[observed_index].timestamp_ms - targets[expected_index];
                if error.abs() > config.boundaries().2 {
                    continue;
                }
                let utility = config.note_score(error).round() as i32
                    + SCORING_INVALID_HIT_PENALTY.round() as i32;
                let exact = usize::from(
                    observed[observed_index].raw_note == Some(expected[expected_index].note),
                );
                plan.utility += utility;
                plan.matches += 1;
                plan.exact += exact;
                plan.error_ns += (error.abs() * 1_000_000.0).round() as i64;
                plan.pairs.push((expected_index, observed_index));
                visit(
                    expected,
                    targets,
                    observed,
                    expected_indices,
                    observed_indices,
                    config,
                    expected_cursor + 1,
                    candidate + 1,
                    plan,
                    best,
                );
                plan.pairs.pop();
                plan.error_ns -= (error.abs() * 1_000_000.0).round() as i64;
                plan.exact -= exact;
                plan.matches -= 1;
                plan.utility -= utility;
            }
        }
        let mut best = ReferencePlan::default();
        visit(
            expected,
            targets,
            observed,
            expected_indices,
            observed_indices,
            config,
            0,
            0,
            &mut ReferencePlan::default(),
            &mut best,
        );
        best
    }

    fn reference_v4_assignments(
        expected: &[ExpectedHit],
        observed: &[ObservedHit],
    ) -> Vec<Option<usize>> {
        let config = ScoringConfig::default();
        let targets = expected
            .iter()
            .map(|hit| elapsed_ms_at_beat(hit.beat, 60.0, &[]))
            .collect::<Vec<_>>();
        let mut expected_order = (0..expected.len()).collect::<Vec<_>>();
        expected_order.sort_by(|left, right| {
            targets[*left]
                .total_cmp(&targets[*right])
                .then(left.cmp(right))
        });
        let mut confirmed = vec![false; expected.len()];
        let mut consumed = vec![false; observed.len()];
        let mut assignments = vec![None; expected.len()];
        for current in expected_order {
            let watermark = targets[current] + 2.0 * config.boundaries().2 + 0.001;
            let instrument = expected[current].instrument;
            let mut observed_indices = observed
                .iter()
                .enumerate()
                .filter(|(index, hit)| {
                    !consumed[*index]
                        && hit.instrument == instrument
                        && hit.timestamp_ms <= watermark
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            observed_indices.sort_by(|left, right| {
                observed[*left]
                    .timestamp_ms
                    .total_cmp(&observed[*right].timestamp_ms)
                    .then(left.cmp(right))
            });
            let mut expected_indices = expected
                .iter()
                .enumerate()
                .filter(|(index, hit)| !confirmed[*index] && hit.instrument == instrument)
                .filter(|(index, _)| {
                    targets[*index] <= targets[current] + 2.0 * config.boundaries().2 + 0.000_001
                })
                .filter(|(index, _)| {
                    observed_indices.iter().any(|observed_index| {
                        (observed[*observed_index].timestamp_ms - targets[*index]).abs()
                            <= config.boundaries().2
                    })
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if !expected_indices.contains(&current) {
                expected_indices.push(current);
            }
            expected_indices.sort_by(|left, right| {
                targets[*left]
                    .total_cmp(&targets[*right])
                    .then(left.cmp(right))
            });
            let plan = reference_local_plan(
                expected,
                &targets,
                observed,
                &expected_indices,
                &observed_indices,
                config,
            );
            if let Some(observed_index) =
                plan.pairs
                    .iter()
                    .find_map(|(expected_index, observed_index)| {
                        (*expected_index == current).then_some(*observed_index)
                    })
            {
                assignments[current] = Some(observed_index);
                consumed[observed_index] = true;
            }
            confirmed[current] = true;
        }
        assignments
    }

    #[test]
    fn v4_incremental_engine_matches_independent_short_sequence_reference() {
        let expected = dense_expected(3, 62.5);
        let candidates = [-100.0, -50.0, 0.0, 50.0, 100.0, 150.0, 200.0, 250.0];
        for first in 0..candidates.len() {
            for second in first..candidates.len() {
                for third in second..candidates.len() {
                    let observed = [first, second, third]
                        .map(|index| obs(candidates[index], Instrument::Snare));
                    let reference = reference_v4_assignments(&expected, &observed)
                        .into_iter()
                        .map(|assignment| assignment.map(|index| observed[index].timestamp_ms))
                        .collect::<Vec<_>>();
                    let summary = score_session(&expected, &observed, 60.0, 0.0, 0.0);
                    let actual = summary.results[..expected.len()]
                        .iter()
                        .map(|result| result.observed_ms)
                        .collect::<Vec<_>>();
                    assert_eq!(actual, reference, "case {first}/{second}/{third}");
                }
            }
        }
    }

    #[test]
    fn v4_score_first_locality_prefers_nine_perfect_hits_over_a_shifted_chain() {
        let expected = dense_expected(10, 62.5);
        let mut observed = expected
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != 3)
            .map(|(_, hit)| obs(hit.beat * 1000.0 + 5.0, Instrument::Snare))
            .collect::<Vec<_>>();
        observed.push(obs(
            expected.last().unwrap().beat * 1000.0 + 70.0,
            Instrument::Snare,
        ));
        let summary = score_session(&expected, &observed, 60.0, 0.0, 0.0);
        assert_eq!(summary.score, Some(85.0));
        assert_eq!(summary.matched, 9);
        assert_eq!(summary.missed, 1);
        assert_eq!(summary.extras, 1);
        assert_eq!(summary.results[3].judgement, Judgement::Missed);
        assert!(
            summary.results[4..10]
                .iter()
                .all(|result| result.judgement == Judgement::Perfect)
        );
    }

    #[test]
    fn v4_miss_confirms_only_after_the_inclusive_late_boundary() {
        let expected = dense_expected(1, 100.0);
        let mut scorer =
            IncrementalScorer::new(&expected, 60.0, 1_000.0, ScoringConfig::default(), &[]);
        scorer.advance_event_time(1_200.0).unwrap();
        assert!(scorer.feedback().unwrap().is_empty());
        scorer.advance_event_time(1_200.001).unwrap();
        let feedback = scorer.feedback().unwrap();
        assert_eq!(feedback.len(), 1);
        assert!(feedback[0].confirmed);
        assert_eq!(feedback[0].result.judgement, Judgement::Missed);
    }

    #[test]
    fn v4_confirmed_miss_cannot_be_reopened_by_late_delivery() {
        let expected = dense_expected(1, 100.0);
        let mut scorer =
            IncrementalScorer::new(&expected, 60.0, 0.0, ScoringConfig::default(), &[]);
        scorer.advance_event_time(200.001).unwrap();
        let error = scorer
            .observe(obs(100.0, Instrument::Snare), 100.0)
            .unwrap_err();
        assert!(matches!(
            error,
            IncrementalScoringError::LateDelivery { .. }
        ));
        assert_eq!(scorer.summary().missed, 1);
    }

    #[test]
    fn v4_projects_unmatched_hits_before_confirming_them() {
        let mut future = dense_expected(1, 100.0);
        future[0].beat = 1.0;
        let mut scorer = IncrementalScorer::new(&future, 60.0, 0.0, ScoringConfig::default(), &[]);
        scorer
            .observe(obs(150.0, Instrument::Snare), 150.0)
            .unwrap();
        let feedback = scorer.feedback().unwrap();
        assert_eq!(feedback.len(), 1);
        assert!(!feedback[0].confirmed);
        assert_eq!(feedback[0].result.judgement, Judgement::Extra);

        let expected = dense_expected(1, 100.0);
        let mut scorer =
            IncrementalScorer::new(&expected, 60.0, 0.0, ScoringConfig::default(), &[]);
        scorer.observe(obs(0.0, Instrument::Kick), 0.0).unwrap();
        let feedback = scorer.feedback().unwrap();
        assert_eq!(feedback.len(), 1);
        assert!(!feedback[0].confirmed);
        assert_eq!(feedback[0].result.judgement, Judgement::WrongInstrument);
    }

    #[test]
    fn v4_batching_does_not_change_admissible_results() {
        let expected = dense_expected(8, 62.5);
        let observed = expected
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != 2)
            .map(|(_, hit)| obs(hit.beat * 1000.0 + 8.0, Instrument::Snare))
            .collect::<Vec<_>>();
        let mut batched =
            IncrementalScorer::new(&expected, 60.0, 0.0, ScoringConfig::default(), &[]);
        for hit in &observed {
            batched.observe(hit.clone(), hit.timestamp_ms).unwrap();
        }
        let batched = batched.finish().unwrap();

        let mut streamed =
            IncrementalScorer::new(&expected, 60.0, 0.0, ScoringConfig::default(), &[]);
        for hit in &observed {
            streamed.observe(hit.clone(), hit.timestamp_ms).unwrap();
            streamed
                .advance_event_time((hit.timestamp_ms - 1.0).max(0.0))
                .unwrap();
        }
        let streamed = streamed.finish().unwrap();
        assert_eq!(streamed, batched);
    }

    #[test]
    fn v4_pathological_simultaneous_cluster_fails_explicitly() {
        let expected = dense_expected(129, 0.0);
        let mut scorer =
            IncrementalScorer::new(&expected, 60.0, 0.0, ScoringConfig::default(), &[]);
        for index in 0..129 {
            let mut hit = obs(0.0, Instrument::Snare);
            hit.raw_note = Some((index % 128) as u8);
            scorer.observe(hit, 0.0).unwrap();
        }
        assert!(matches!(
            scorer.feedback(),
            Err(IncrementalScoringError::Overloaded {
                expected: 129,
                observed: 129
            })
        ));
        assert!(scorer.is_overloaded());
    }
}
