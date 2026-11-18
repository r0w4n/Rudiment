//! Stage 1 experiment only. This is not used by the application.
//!
//! Groups intersecting expected-hit windows and delegates their matching to the
//! existing offline scorer. It intentionally exposes the cost of waiting for
//! complete connected groups; it is not a production-ready streaming engine.

use rudiment_core::{
    ExpectedHit, HitResult, InputSource, Instrument, ObservedHit, ScoringConfig, SessionSummary,
    TempoChange, elapsed_ms_at_beat, paradiddle,
    score_session_v3_with_tempo_map as score_session_with_tempo_map,
};
use std::collections::HashMap;
use std::time::Instant;

#[derive(Default)]
struct Group {
    start: f64,
    end: f64,
    expected: Vec<ExpectedHit>,
    targets: Vec<f64>,
    observed: Vec<ObservedHit>,
}

struct Study {
    groups: Vec<Group>,
    cursor: usize,
    bpm: f64,
    start: f64,
    latency: f64,
    config: ScoringConfig,
    tempo: Vec<TempoChange>,
    confirmed: Vec<HitResult>,
    peak_pending: usize,
    last_input: f64,
    watermark: f64,
    expected_order: HashMap<String, usize>,
    input_count: usize,
}

impl Study {
    fn new(
        expected: &[ExpectedHit],
        bpm: f64,
        start: f64,
        latency: f64,
        config: ScoringConfig,
        tempo: &[TempoChange],
    ) -> Self {
        let expected_order: HashMap<_, _> = expected
            .iter()
            .enumerate()
            .map(|(i, hit)| (hit.id.clone(), i))
            .collect();
        assert_eq!(
            expected_order.len(),
            expected.len(),
            "expected IDs must be unique"
        );
        let window = config
            .match_ms
            .max(config.good_ms)
            .max(config.perfect_ms)
            .max(0.0);
        let mut ordered = expected.to_vec();
        ordered.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        let mut groups: Vec<Group> = vec![];
        for hit in ordered {
            let target = start + elapsed_ms_at_beat(hit.beat, bpm, tempo);
            if groups
                .last()
                .is_none_or(|group| target - window > group.end)
            {
                groups.push(Group {
                    start: target - window,
                    end: target + window,
                    ..Group::default()
                });
            }
            let group = groups.last_mut().unwrap();
            group.end = group.end.max(target + window);
            group.targets.push(target);
            group.expected.push(hit);
        }
        Self {
            groups,
            cursor: 0,
            bpm,
            start,
            latency,
            config,
            tempo: tempo.to_vec(),
            confirmed: vec![],
            peak_pending: 0,
            last_input: f64::NEG_INFINITY,
            watermark: f64::NEG_INFINITY,
            expected_order,
            input_count: 0,
        }
    }

    // The caller must guarantee no later delivery with an earlier timestamp.
    // A real Web MIDI integration has not established that watermark contract.
    fn advance(&mut self, calibrated_watermark: f64) {
        assert!(
            calibrated_watermark >= self.watermark,
            "watermark must not retreat"
        );
        self.watermark = calibrated_watermark;
        while self.cursor < self.groups.len() && self.groups[self.cursor].end < calibrated_watermark
        {
            let group = &mut self.groups[self.cursor];
            let summary = score_session_with_tempo_map(
                &group.expected,
                &group.observed,
                self.bpm,
                self.start,
                self.latency,
                self.config,
                &self.tempo,
            );
            self.confirmed.extend(summary.results);
            group.observed.clear();
            group.observed.shrink_to_fit();
            self.cursor += 1;
        }
    }

    // Provisional projection only: score the current hit's small local time
    // range with the existing policy, not the entire unresolved chain. It may
    // be revised when the connected group is finally evaluated.
    fn hit(&mut self, hit: ObservedHit) -> Option<HitResult> {
        assert!(
            hit.timestamp_ms >= self.last_input,
            "study requires ordered delivery"
        );
        assert!(
            hit.timestamp_ms - self.latency >= self.watermark,
            "event precedes watermark"
        );
        self.last_input = hit.timestamp_ms;
        self.input_count += 1;
        let time = hit.timestamp_ms - self.latency;
        self.advance(time);
        let Some(group) = self.groups.get_mut(self.cursor) else {
            self.confirmed.push(extra(&hit));
            return Some(extra(&hit));
        };
        if time < group.start {
            self.confirmed.push(extra(&hit));
            return Some(extra(&hit));
        }
        group.observed.push(hit.clone());
        self.peak_pending = self.peak_pending.max(group.observed.len());
        let window = self
            .config
            .match_ms
            .max(self.config.good_ms)
            .max(self.config.perfect_ms)
            .max(0.0);
        let first = group
            .targets
            .partition_point(|target| *target < time - window);
        let last = group
            .targets
            .partition_point(|target| *target <= time + window);
        let first_observed = group
            .observed
            .partition_point(|observed| observed.timestamp_ms - self.latency < time - 2.0 * window);
        let summary = score_session_with_tempo_map(
            &group.expected[first..last],
            &group.observed[first_observed..],
            self.bpm,
            self.start,
            self.latency,
            self.config,
            &self.tempo,
        );
        // Timestamp collisions cannot identify the latest event reliably. The
        // prototype deliberately does not pretend otherwise.
        summary
            .results
            .into_iter()
            .find(|result| result.observed_ms == Some(hit.timestamp_ms))
    }

    fn finish(&mut self) {
        self.advance(f64::INFINITY);
    }

    // These are only *candidate* confirmations: the offline tie-break
    // counterexample in the tests means this summary is not authoritative.
    fn summary(&self) -> SessionSummary {
        assert_eq!(self.watermark, f64::INFINITY);
        let expected = self.expected_order.len();
        if expected == 0 {
            return SessionSummary {
                score: None,
                matched: 0,
                expected: 0,
                missed: 0,
                extras: self.input_count,
                wrong_instrument: 0,
                mean_abs_error_ms: None,
                signed_mean_error_ms: None,
                timing_consistency_ms: None,
                scoring_version: "v3".into(),
                results: vec![],
            };
        }
        let mut results = self.confirmed.clone();
        results.sort_by(|a, b| match (&a.expected_id, &b.expected_id) {
            (Some(a), Some(b)) => self.expected_order[a].cmp(&self.expected_order[b]),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.observed_ms.unwrap().total_cmp(&b.observed_ms.unwrap()),
        });
        let errors: Vec<f64> = results
            .iter()
            .take(expected)
            .filter_map(|result| result.error_ms)
            .collect();
        let matched = errors.len();
        let extras = results
            .iter()
            .filter(|result| result.judgement == rudiment_core::Judgement::Extra)
            .count();
        let wrong_instrument = results
            .iter()
            .filter(|result| result.judgement == rudiment_core::Judgement::WrongInstrument)
            .count();
        let points: f64 = results
            .iter()
            .take(expected)
            .map(|result| result.note_score)
            .sum();
        let score = (((points - 50.0 * (extras + wrong_instrument) as f64) / expected as f64)
            .clamp(0.0, 100.0)
            * 10.0)
            .round()
            / 10.0;
        let mean_abs_error_ms = (matched > 0)
            .then(|| errors.iter().map(|error| error.abs()).sum::<f64>() / matched as f64);
        let signed_mean_error_ms =
            (matched > 0).then(|| errors.iter().sum::<f64>() / matched as f64);
        let timing_consistency_ms = signed_mean_error_ms.map(|mean| {
            (errors
                .iter()
                .map(|error| (error - mean).powi(2))
                .sum::<f64>()
                / matched as f64)
                .sqrt()
        });
        SessionSummary {
            score: Some(score),
            matched,
            expected,
            missed: expected - matched,
            extras,
            wrong_instrument,
            mean_abs_error_ms,
            signed_mean_error_ms,
            timing_consistency_ms,
            scoring_version: "v3".into(),
            results,
        }
    }
}

fn extra(hit: &ObservedHit) -> HitResult {
    HitResult {
        expected_id: None,
        expected_beat: None,
        expected_timestamp_ms: None,
        observed_ms: Some(hit.timestamp_ms),
        actual_timestamp_ms: Some(hit.timestamp_ms),
        error_ms: None,
        window_ms: 0.0,
        instrument: hit.instrument,
        midi_note: hit.raw_note,
        judgement: rudiment_core::Judgement::Extra,
        excellent: false,
        note_score: 0.0,
        missed: false,
    }
}

fn expected(count: usize, spacing_ms: f64) -> Vec<ExpectedHit> {
    let template = paradiddle().hits[0].clone();
    (0..count)
        .map(|index| ExpectedHit {
            id: format!("n{index}-loop{}", index / 16 + 1),
            beat: index as f64 * spacing_ms / 1000.0,
            notation_beat: index as f64 * spacing_ms / 1000.0,
            ..template.clone()
        })
        .collect()
}

fn observed(timestamp_ms: f64) -> ObservedHit {
    ObservedHit {
        timestamp_ms,
        instrument: Instrument::Snare,
        articulation: Default::default(),
        velocity: 100,
        source: InputSource::Midi,
        raw_note: Some(38),
        raw_channel: Some(10),
    }
}

fn percentile(samples: &[f64], percent: f64) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) as f64 * percent).ceil() as usize]
}

fn run(count: usize, spacing: f64) {
    let expected = expected(count, spacing);
    let config = ScoringConfig::default();
    let setup = Instant::now();
    let mut study = Study::new(&expected, 60.0, 0.0, 0.0, config, &[]);
    let setup_ms = setup.elapsed().as_secs_f64() * 1000.0;
    let groups = study.groups.len();
    let max_group = study
        .groups
        .iter()
        .map(|g| g.expected.len())
        .max()
        .unwrap_or(0);
    let mut times = Vec::with_capacity(count);
    for index in 0..count {
        let start = Instant::now();
        std::hint::black_box(study.hit(observed(index as f64 * spacing + 5.0)));
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let start = Instant::now();
    study.finish();
    let finish_ms = start.elapsed().as_secs_f64() * 1000.0;
    std::hint::black_box(study.summary());
    let observed: Vec<_> = (0..count)
        .map(|i| observed(i as f64 * spacing + 5.0))
        .collect();
    let start = Instant::now();
    std::hint::black_box(score_session_with_tempo_map(
        &expected,
        &observed,
        60.0,
        0.0,
        0.0,
        config,
        &[],
    ));
    let offline_ms = start.elapsed().as_secs_f64() * 1000.0;
    println!(
        "count={count} spacing_ms={spacing:.1} groups={groups} max_group={max_group} peak_pending={} setup_ms={setup_ms:.3} hit_p50_ms={:.4} hit_p95_ms={:.4} hit_p99_ms={:.4} hit_max_ms={:.4} finish_ms={finish_ms:.3} offline_ms={offline_ms:.3}",
        study.peak_pending,
        percentile(&times, 0.50),
        percentile(&times, 0.95),
        percentile(&times, 0.99),
        percentile(&times, 1.0)
    );
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 2 {
        run(
            args[0].parse().expect("count"),
            args[1].parse().expect("spacing ms"),
        );
        return;
    }
    for (count, spacing) in [
        (500, 1000.0),
        (500, 250.0),
        (500, 125.0),
        (500, 62.5),
        (1000, 31.25),
        (2000, 62.5),
    ] {
        run(count, spacing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compare(
        expected: &[ExpectedHit],
        observed: &[ObservedHit],
        latency: f64,
        tempo: &[TempoChange],
    ) -> bool {
        let config = ScoringConfig::default();
        let offline =
            score_session_with_tempo_map(expected, observed, 60.0, 0.0, latency, config, tempo);
        let mut study = Study::new(expected, 60.0, 0.0, latency, config, tempo);
        for hit in observed {
            study.hit(hit.clone());
        }
        study.finish();
        study.summary() == offline
    }

    #[test]
    fn exhaustive_small_ordered_sequences_expose_tie_break_divergence() {
        let mut divergences = 0;
        let mut total = 0;
        for spacing in [50.0, 100.0, 200.0, 250.0] {
            let expected = expected(3, spacing);
            let candidates = [
                -101.0, -50.0, 0.0, 50.0, 100.0, 150.0, 200.0, 300.0, 500.0, 800.0,
            ];
            for a in 0..candidates.len() {
                for b in a..candidates.len() {
                    for c in b..candidates.len() {
                        total += 1;
                        if !compare(
                            &expected,
                            &[
                                observed(candidates[a]),
                                observed(candidates[b]),
                                observed(candidates[c]),
                            ],
                            0.0,
                            &[],
                        ) {
                            divergences += 1;
                        }
                    }
                }
            }
        }
        eprintln!("{divergences} assignment/result divergences in {total} cases");
        assert!(divergences > 0);
    }

    #[test]
    fn tempo_calibration_and_wrong_instrument_match_offline() {
        let mut expected = expected(4, 250.0);
        expected[1].instrument = Instrument::Kick;
        let tempo = [
            TempoChange {
                tick: 0,
                beat: 0.0,
                bpm: 60.0,
            },
            TempoChange {
                tick: 480,
                beat: 0.5,
                bpm: 120.0,
            },
        ];
        let mut hits = vec![
            observed(30.0),
            observed(280.0),
            observed(405.0),
            observed(530.0),
        ];
        hits[1].instrument = Instrument::Kick;
        assert!(compare(&expected, &hits, 30.0, &tempo));
    }

    #[test]
    fn late_boundary_and_final_note_are_not_missed_early() {
        let notes = expected(1, 1000.0);
        let mut study = Study::new(&notes, 60.0, 0.0, 30.0, ScoringConfig::default(), &[]);
        study.advance(100.0);
        assert!(study.confirmed.is_empty());
        study.hit(observed(130.0));
        study.advance(100.001);
        assert_eq!(study.confirmed[0].judgement, rudiment_core::Judgement::Late);
        study.finish();
        assert_eq!(study.summary().missed, 0);

        let mut silent = Study::new(&notes, 60.0, 0.0, 30.0, ScoringConfig::default(), &[]);
        silent.advance(100.0);
        assert!(silent.confirmed.is_empty());
        silent.advance(100.001);
        assert_eq!(
            silent.confirmed[0].judgement,
            rudiment_core::Judgement::Missed
        );
        silent.finish();
        assert_eq!(silent.summary().missed, 1);
    }

    #[test]
    fn grace_cluster_and_repeated_ids_compare_on_the_expanded_timeline() {
        let mut notes = expected(6, 250.0);
        for (index, beat) in [0.0, 0.03, 0.06, 0.25, 4.0, 4.03].into_iter().enumerate() {
            notes[index].beat = beat;
            notes[index].notation_beat = beat;
            notes[index].grace = matches!(index, 0 | 1 | 4);
            notes[index].id = format!("{}-loop{}", index % 4, if index < 4 { 1 } else { 2 });
        }
        let hits = [5.0, 32.0, 64.0, 254.0, 4004.0, 4034.0].map(observed);
        assert!(compare(&notes, &hits, 0.0, &[]));
    }

    #[test]
    fn outside_extra_changes_global_tie_break() {
        let notes = expected(3, 50.0);
        let hits = [observed(-101.0), observed(50.0), observed(50.0)];
        assert!(!compare(&notes, &hits, 0.0, &[]));
        assert!(compare(&notes, &hits[1..], 0.0, &[]));
    }

    #[test]
    fn overlapping_chain_prevents_early_confirmation() {
        let expected = expected(1000, 62.5);
        let mut study = Study::new(&expected, 60.0, 0.0, 0.0, ScoringConfig::default(), &[]);
        for i in 0..1000 {
            study.hit(observed(i as f64 * 62.5));
        }
        assert_eq!(study.groups.len(), 1);
        assert_eq!(study.peak_pending, 1000);
        assert!(study.confirmed.is_empty());
        study.finish();
        assert_eq!(study.confirmed.len(), 1000);
    }
}
