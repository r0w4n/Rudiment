use super::*;
use engine::Confirmation;
use rudiment_core::{
    Judgement, TempoChange, elapsed_ms_at_beat,
    score_session_v3_with_tempo_map as score_session_with_tempo_map,
};

#[derive(Clone, Default)]
struct RefPlan {
    exact: usize,
    error_ns: u128,
    pairs: Vec<(usize, usize)>,
}

fn preferred(a: &RefPlan, b: &RefPlan) -> bool {
    a.pairs.len() > b.pairs.len()
        || (a.pairs.len() == b.pairs.len()
            && (a.exact > b.exact
                || (a.exact == b.exact
                    && (a.error_ns < b.error_ns
                        || (a.error_ns == b.error_ns && a.pairs < b.pairs)))))
}

// Exhaustive offline oracle: enumerate all ordered one-to-one assignments.
// It shares no frontier, Fenwick tree, prefix-settlement or traceback code.
fn reference(
    expected: &[ExpectedHit],
    observed: &[ObservedHit],
    bpm: f64,
    start: f64,
    latency: f64,
    tempo: &[TempoChange],
) -> Vec<Option<usize>> {
    let mut instruments = vec![];
    for note in expected {
        if !instruments.contains(&note.instrument) {
            instruments.push(note.instrument);
        }
    }
    let mut assignments = vec![None; expected.len()];
    for instrument in instruments {
        let mut targets: Vec<_> = expected
            .iter()
            .enumerate()
            .filter(|(_, note)| note.instrument == instrument)
            .map(|(index, note)| {
                (
                    index,
                    note,
                    start + elapsed_ms_at_beat(note.beat, bpm, tempo),
                )
            })
            .collect();
        targets.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)));
        let hits: Vec<_> = observed
            .iter()
            .enumerate()
            .filter(|(_, hit)| hit.instrument == instrument)
            .collect();
        fn enumerate(
            targets: &[(usize, &ExpectedHit, f64)],
            hits: &[(usize, &ObservedHit)],
            latency: f64,
            note: usize,
            first_hit: usize,
            current: &mut RefPlan,
            best: &mut Option<RefPlan>,
        ) {
            if note == targets.len() {
                if best.as_ref().is_none_or(|old| preferred(current, old)) {
                    *best = Some(current.clone());
                }
                return;
            }
            enumerate(targets, hits, latency, note + 1, first_hit, current, best);
            for hit in first_hit..hits.len() {
                let error = hits[hit].1.timestamp_ms - latency - targets[note].2;
                if !error.is_finite() || error.abs() > 100.0 {
                    continue;
                }
                let exact = usize::from(hits[hit].1.raw_note == Some(targets[note].1.note));
                let cost = (error.abs() * 1_000_000.0).round() as u128;
                current.exact += exact;
                current.error_ns += cost;
                current.pairs.push((note, hits[hit].0));
                enumerate(targets, hits, latency, note + 1, hit + 1, current, best);
                current.pairs.pop();
                current.exact -= exact;
                current.error_ns -= cost;
            }
        }
        let mut best = None;
        enumerate(
            &targets,
            &hits,
            latency,
            0,
            0,
            &mut RefPlan::default(),
            &mut best,
        );
        for (index, observed) in best.unwrap().pairs {
            assignments[targets[index].0] = Some(observed);
        }
    }
    assignments
}

fn check_event(
    event: &Confirmation,
    assignments: &[Option<usize>],
    expected: &[ExpectedHit],
    observed: &[ObservedHit],
    timing: (f64, f64, f64, &[TempoChange]),
) {
    let (bpm, start, latency, tempo) = timing;
    if let Some(index) = event.expected_index {
        assert_eq!(
            event.expected_id.as_deref(),
            Some(expected[index].id.as_str())
        );
        assert_eq!(
            event.observed_index, assignments[index],
            "changed confirmed assignment for {}",
            expected[index].id
        );
        let target = start + elapsed_ms_at_beat(expected[index].beat, bpm, tempo);
        assert_eq!(event.expected_ms, Some(target));
        let error = assignments[index].map(|hit| observed[hit].timestamp_ms - latency - target);
        assert_eq!(event.error_ms, error);
        assert_eq!(
            event.judgement,
            error
                .map(|value| ScoringConfig::default().classify(value))
                .unwrap_or(Judgement::Missed)
        );
    } else {
        let index = event.observed_index.unwrap();
        assert!(
            !assignments.contains(&Some(index)),
            "confirmed extra was later matched"
        );
        let hit = &observed[index];
        let wrong = expected.iter().any(|target| {
            target.instrument != hit.instrument
                && (hit.timestamp_ms
                    - latency
                    - (start + elapsed_ms_at_beat(target.beat, bpm, tempo)))
                .abs()
                    <= 100.0
        });
        assert_eq!(
            event.judgement,
            if wrong {
                Judgement::WrongInstrument
            } else {
                Judgement::Extra
            }
        );
    }
}

fn compare(
    expected: &[ExpectedHit],
    observed: &[ObservedHit],
    bpm: f64,
    start: f64,
    latency: f64,
    tempo: &[TempoChange],
) -> Frontier {
    let expected_assignments = reference(expected, observed, bpm, start, latency, tempo);
    let mut scorer = Frontier::new(
        expected,
        bpm,
        start,
        latency,
        ScoringConfig::default(),
        tempo,
        (MAX_WORK, MAX_NODES),
    )
    .unwrap();
    let mut checked = 0;
    for input in observed {
        scorer.push(input.clone()).unwrap();
        for event in &scorer.confirmed[checked..] {
            check_event(
                event,
                &expected_assignments,
                expected,
                observed,
                (bpm, start, latency, tempo),
            );
        }
        checked = scorer.confirmed.len();
    }
    scorer.finish().unwrap();
    for event in &scorer.confirmed[checked..] {
        check_event(
            event,
            &expected_assignments,
            expected,
            observed,
            (bpm, start, latency, tempo),
        );
    }
    for index in 0..expected.len() {
        assert_eq!(
            scorer
                .confirmed
                .iter()
                .filter(|event| event.expected_index == Some(index))
                .count(),
            1
        );
    }
    for index in 0..observed.len() {
        assert_eq!(
            scorer
                .confirmed
                .iter()
                .filter(|event| event.observed_index == Some(index))
                .count(),
            1
        );
    }
    let summary = scorer.summary().unwrap();
    let errors: Vec<_> = expected_assignments
        .iter()
        .enumerate()
        .filter_map(|(index, actual)| {
            actual.map(|actual| {
                observed[actual].timestamp_ms
                    - latency
                    - (start + elapsed_ms_at_beat(expected[index].beat, bpm, tempo))
            })
        })
        .collect();
    let invalid: Vec<_> = observed
        .iter()
        .enumerate()
        .filter(|(index, _)| !expected_assignments.contains(&Some(*index)))
        .collect();
    let wrong = invalid
        .iter()
        .filter(|(_, hit)| {
            expected.iter().any(|target| {
                target.instrument != hit.instrument
                    && (hit.timestamp_ms
                        - latency
                        - (start + elapsed_ms_at_beat(target.beat, bpm, tempo)))
                    .abs()
                        <= 100.0
            })
        })
        .count();
    let points: f64 = errors
        .iter()
        .map(|error| ScoringConfig::default().note_score(*error))
        .sum();
    let score = (!expected.is_empty()).then(|| {
        (((points - 50.0 * invalid.len() as f64) / expected.len() as f64).clamp(0.0, 100.0) * 10.0)
            .round()
            / 10.0
    });
    let signed = (!errors.is_empty()).then(|| errors.iter().sum::<f64>() / errors.len() as f64);
    let mean_abs = (!errors.is_empty())
        .then(|| errors.iter().map(|error| error.abs()).sum::<f64>() / errors.len() as f64);
    let consistency = signed.map(|mean| {
        (errors
            .iter()
            .map(|error| (error - mean).powi(2))
            .sum::<f64>()
            / errors.len() as f64)
            .sqrt()
    });
    assert_eq!(
        (
            summary.score,
            summary.matched,
            summary.missed,
            summary.extras,
            summary.wrong_instrument
        ),
        (
            score,
            errors.len(),
            expected.len() - errors.len(),
            invalid.len() - wrong,
            wrong
        )
    );
    assert_eq!(
        (
            summary.mean_abs_error_ms,
            summary.signed_mean_error_ms,
            summary.timing_consistency_ms
        ),
        (mean_abs, signed, consistency)
    );
    assert_eq!(summary.scoring_version, engine::PROPOSED_VERSION);
    scorer
}

#[test]
fn all_original_880_cases_match_the_independent_candidate_policy() {
    let candidates = [
        -101.0, -50.0, 0.0, 50.0, 100.0, 150.0, 200.0, 300.0, 500.0, 800.0,
    ];
    let mut count = 0;
    for spacing in [50.0, 100.0, 200.0, 250.0] {
        let expected = notes(3, spacing);
        for a in 0..candidates.len() {
            for b in a..candidates.len() {
                for c in b..candidates.len() {
                    compare(
                        &expected,
                        &[hit(candidates[a]), hit(candidates[b]), hit(candidates[c])],
                        60.0,
                        0.0,
                        0.0,
                        &[],
                    );
                    count += 1;
                }
            }
        }
    }
    assert_eq!(count, 880);
}

#[test]
fn report_candidate_differences_from_historical_scoring() {
    let candidates = [
        -101.0, -50.0, 0.0, 50.0, 100.0, 150.0, 200.0, 300.0, 500.0, 800.0,
    ];
    let (mut assignments_changed, mut scores_changed) = (0, 0);
    for spacing in [50.0, 100.0, 200.0, 250.0] {
        let expected = notes(3, spacing);
        for a in 0..candidates.len() {
            for b in a..candidates.len() {
                for c in b..candidates.len() {
                    let observed = [hit(candidates[a]), hit(candidates[b]), hit(candidates[c])];
                    let proposed = compare(&expected, &observed, 60.0, 0.0, 0.0, &[]);
                    let legacy = score_session_with_tempo_map(
                        &expected,
                        &observed,
                        60.0,
                        0.0,
                        0.0,
                        ScoringConfig::default(),
                        &[],
                    );
                    let mut timestamps = vec![None; expected.len()];
                    for result in &proposed.confirmed {
                        if let Some(index) = result.expected_index {
                            timestamps[index] =
                                result.observed_index.map(|hit| observed[hit].timestamp_ms);
                        }
                    }
                    if legacy
                        .results
                        .iter()
                        .take(expected.len())
                        .map(|result| result.observed_ms)
                        .collect::<Vec<_>>()
                        != timestamps
                    {
                        assignments_changed += 1;
                    }
                    if legacy.score != proposed.summary().unwrap().score {
                        scores_changed += 1;
                    }
                }
            }
        }
    }
    eprintln!(
        "candidate versus v3: {assignments_changed} cases with different note/timestamp assignments, {scores_changed} with different scores (880 cases)"
    );
    assert_eq!(assignments_changed, 23);
    assert_eq!(scores_changed, 0);
}

#[test]
fn irrelevant_extra_does_not_change_the_candidate_tie_but_differs_from_v3() {
    let expected = notes(3, 50.0);
    let observed = [hit(-101.0), hit(50.0), hit(50.0)];
    let before = reference(&expected, &observed[1..], 60.0, 0.0, 0.0, &[]);
    let after = reference(&expected, &observed, 60.0, 0.0, 0.0, &[]);
    assert_eq!(before, vec![Some(0), Some(1), None]);
    assert_eq!(after, vec![Some(1), Some(2), None]);
    let legacy = score_session_with_tempo_map(
        &expected,
        &observed,
        60.0,
        0.0,
        0.0,
        ScoringConfig::default(),
        &[],
    );
    assert!(legacy.results[0].missed);
    assert_eq!(legacy.results[2].observed_ms, Some(50.0));
    compare(&expected, &observed, 60.0, 0.0, 0.0, &[]);
}

#[test]
fn one_later_hit_can_change_an_already_closed_note_under_the_primary_objective() {
    let expected = notes(4, 62.5);
    let prefix = [hit(5.0), hit(130.0), hit(192.5)];
    let completed = [hit(5.0), hit(130.0), hit(192.5), hit(193.5)];
    assert_eq!(
        reference(&expected, &prefix, 60.0, 0.0, 0.0, &[]),
        vec![Some(0), None, Some(1), Some(2)]
    );
    assert_eq!(
        reference(&expected, &completed, 60.0, 0.0, 0.0, &[]),
        vec![Some(0), Some(1), Some(2), Some(3)]
    );
    let mut scorer = make(&expected);
    for event in &prefix {
        scorer.push(event.clone()).unwrap();
    }
    assert!(prefix.last().unwrap().timestamp_ms > expected[1].beat * 1000.0 + 100.0);
    assert!(
        !scorer
            .confirmed
            .iter()
            .any(|event| event.expected_index == Some(1))
    );
    compare(&expected, &prefix, 60.0, 0.0, 0.0, &[]);
    compare(&expected, &completed, 60.0, 0.0, 0.0, &[]);
}

#[test]
fn maximum_cardinality_can_score_worse_than_a_musically_local_assignment() {
    let expected = notes(10, 62.5);
    let mut observed: Vec<_> = (0..10)
        .filter(|index| *index != 1)
        .map(|index| hit(index as f64 * 62.5 + 5.0))
        .collect();
    observed.push(hit(observed.last().unwrap().timestamp_ms + 1.0));
    let legacy = score_session_with_tempo_map(
        &expected,
        &observed,
        60.0,
        0.0,
        0.0,
        ScoringConfig::default(),
        &[],
    );
    assert_eq!(legacy.matched, 10);
    assert_eq!(legacy.score, Some(68.0));
    let candidate = compare(&expected, &observed, 60.0, 0.0, 0.0, &[]);
    assert_eq!(candidate.summary().unwrap().score, Some(68.0));
    // Alternative: nine Perfect hits, one miss and one extra, using the same
    // v3 points and invalid-hit penalty. It is not the current matching policy.
    let local_assignment_score = ((9.0_f64 * 100.0 - 50.0) / 10.0 * 10.0).round() / 10.0;
    assert_eq!(local_assignment_score, 85.0);
}

#[test]
fn nanosecond_tie_can_change_a_boundary_judgement_from_v3() {
    let mut expected = notes(2, 0.0);
    expected[1].beat = 1e-13; // target 0.0000000001 ms after the first
    let observed = [hit(20.000_000_000_1)];
    let candidate = compare(&expected, &observed, 60.0, 0.0, 0.0, &[]);
    let first = candidate
        .confirmed
        .iter()
        .find(|event| event.expected_index == Some(0))
        .unwrap();
    assert_eq!(first.judgement, Judgement::Good);
    let legacy = score_session_with_tempo_map(
        &expected,
        &observed,
        60.0,
        0.0,
        0.0,
        ScoringConfig::default(),
        &[],
    );
    assert_eq!(legacy.results[1].judgement, Judgement::Perfect);
    assert_ne!(candidate.summary().unwrap().score, legacy.score);
}

#[test]
fn exact_raw_note_still_precedes_timing_error_in_the_candidate_policy() {
    let mut expected = notes(2, 80.0);
    expected[1].note = 40;
    let mut event = hit(10.0);
    event.raw_note = Some(40);
    let scorer = compare(&expected, &[event], 60.0, 0.0, 0.0, &[]);
    assert_eq!(
        scorer
            .confirmed
            .iter()
            .find(|result| result.expected_index == Some(1))
            .unwrap()
            .judgement,
        Judgement::Early
    );
}

#[test]
fn randomized_multi_instrument_collisions_and_extras_match_reference() {
    let mut random = 0x7a11_cafe_1234_5678u64;
    let mut next = || {
        random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
        random >> 32
    };
    for case in 0..1000 {
        let n = (next() % 7) as usize;
        let m = (next() % 7) as usize;
        let mut expected = notes(n, 50.0);
        for note in &mut expected {
            // Deliberately unsorted and sometimes simultaneous, including
            // 25 ms grace-like clusters and equal expected timestamps.
            note.beat = (next() % 8) as f64 * 0.025;
            note.notation_beat = note.beat;
            if next() % 3 == 0 {
                note.instrument = Instrument::Kick;
                note.note = 36;
            }
            if next() % 3 == 0 {
                note.note = 40;
            }
            note.grace = next() % 4 == 0;
        }
        let mut observed = vec![];
        for _ in 0..m {
            let mut item = hit((next() % 12) as f64 * 50.0 - 100.0);
            if next() % 3 == 0 {
                item.instrument = Instrument::Kick;
            }
            item.raw_note = [None, Some(38), Some(40), Some(36)][(next() % 4) as usize];
            observed.push(item);
        }
        observed.sort_by(|a, b| a.timestamp_ms.total_cmp(&b.timestamp_ms));
        let _ = case;
        compare(&expected, &observed, 60.0, 0.0, 0.0, &[]);
    }
}

#[test]
fn strict_boundary_and_final_note_miss_are_settled_only_after_the_window() {
    let expected = notes(1, 1000.0);
    let mut scorer = make(&expected);
    scorer.advance(100.0).unwrap();
    assert!(scorer.confirmed.is_empty());
    scorer.push(hit(100.0)).unwrap();
    scorer.advance(100.001).unwrap();
    assert_eq!(scorer.confirmed[0].judgement, Judgement::Late);
    scorer.finish().unwrap();

    let mut silent = make(&expected);
    silent.advance(100.0).unwrap();
    assert!(silent.confirmed.is_empty());
    silent.advance(100.001).unwrap();
    assert_eq!(silent.confirmed[0].judgement, Judgement::Missed);
    let outside = compare(&expected, &[hit(100.0_f64.next_up())], 60.0, 0.0, 0.0, &[]);
    assert_eq!(outside.summary().unwrap().missed, 1);
    assert_eq!(outside.summary().unwrap().extras, 1);
}

#[test]
fn dense_continuous_passage_confirms_prefixes_before_the_end() {
    let expected = notes(100, 62.5);
    let mut scorer = make(&expected);
    for i in 0..100 {
        scorer.push(hit(i as f64 * 62.5 + 5.0)).unwrap();
    }
    assert!(
        scorer
            .confirmed
            .iter()
            .filter(|event| event.expected_index.is_some())
            .count()
            >= 95
    );
    scorer.finish().unwrap();
    assert_eq!(
        scorer
            .confirmed
            .iter()
            .filter(|event| event.expected_index.is_some())
            .count(),
        100
    );
}

#[test]
fn provisional_assignments_are_revised_before_confirmation() {
    let expected = notes(2, 80.0);
    let mut scorer = make(&expected);
    let first = scorer.push(hit(60.0)).unwrap();
    assert!(
        first
            .iter()
            .any(|event| event.observed_index == 0 && event.expected_index == Some(1))
    );
    let second = scorer.push(hit(150.0)).unwrap();
    assert!(
        second
            .iter()
            .any(|event| event.observed_index == 0 && event.expected_index == Some(0))
    );
    assert!(
        second
            .iter()
            .any(|event| event.observed_index == 1 && event.expected_index == Some(1))
    );
    scorer.finish().unwrap();
    let assignments: Vec<_> = scorer
        .confirmed
        .iter()
        .filter(|event| event.expected_index.is_some())
        .map(|event| (event.expected_index, event.observed_index))
        .collect();
    assert_eq!(assignments, vec![(Some(0), Some(0)), (Some(1), Some(1))]);
}

#[test]
fn a_dense_gap_can_remain_unresolved_because_a_later_extra_changes_the_optimum() {
    let expected = notes(64, 62.5);
    let observed: Vec<_> = (0..64)
        .filter(|index| ![3, 10, 28].contains(index))
        .map(|index| hit(index as f64 * 62.5 + 5.0))
        .collect();
    let reference: Vec<_> = (0..64)
        .map(|index| {
            observed
                .iter()
                .position(|hit| hit.timestamp_ms == index as f64 * 62.5 + 5.0)
        })
        .collect();
    let legacy = score_session_with_tempo_map(
        &expected,
        &observed,
        60.0,
        0.0,
        0.0,
        ScoringConfig::default(),
        &[],
    );
    for (index, result) in legacy.results.iter().take(expected.len()).enumerate() {
        assert_eq!(
            result.observed_ms,
            reference[index].map(|hit| observed[hit].timestamp_ms)
        );
    }
    let mut scorer = make(&expected);
    for (index, input) in observed.iter().enumerate() {
        scorer.push(input.clone()).unwrap();
        if index + 1 < observed.len() {
            let frame = (input.timestamp_ms + 16.0).min(observed[index + 1].timestamp_ms);
            scorer.advance(frame).unwrap();
        }
        for event in &scorer.confirmed {
            check_event(
                event,
                &reference,
                &expected,
                &observed,
                (60.0, 0.0, 0.0, &[]),
            );
        }
    }
    let misses: Vec<_> = scorer
        .confirmed
        .iter()
        .filter(|event| event.judgement == Judgement::Missed)
        .map(|event| event.expected_index.unwrap())
        .collect();
    assert_eq!(misses, vec![3, 10]);
    assert_eq!(
        scorer
            .confirmed
            .iter()
            .filter(|event| event.expected_index.is_some())
            .count(),
        28
    );
    assert_eq!(scorer.pending(), 36);
    // The remaining miss is genuinely unstable: an additional hit near the
    // final note permits a long shift and increases v3 match cardinality.
    assert_eq!(legacy.results[28].judgement, Judgement::Missed);
    let mut with_extra = observed.clone();
    with_extra.push(hit(observed.last().unwrap().timestamp_ms + 1.0));
    let completed = score_session_with_tempo_map(
        &expected,
        &with_extra,
        60.0,
        0.0,
        0.0,
        ScoringConfig::default(),
        &[],
    );
    assert_eq!(completed.matched, legacy.matched + 1);
    assert_ne!(completed.results[28].judgement, Judgement::Missed);
    scorer.push(with_extra.last().unwrap().clone()).unwrap();
    scorer.finish().unwrap();
    assert_eq!(scorer.summary().unwrap().missed, 2);
    assert_ne!(
        scorer
            .confirmed
            .iter()
            .find(|event| event.expected_index == Some(28))
            .unwrap()
            .judgement,
        Judgement::Missed
    );
}

#[test]
fn grace_tempo_calibration_and_repetition_keep_ids() {
    let mut expected = notes(6, 250.0);
    for (index, beat) in [0.0, 0.03, 0.06, 0.25, 4.0, 4.03].into_iter().enumerate() {
        expected[index].beat = beat;
        expected[index].grace = matches!(index, 0 | 1 | 4);
        expected[index].id = format!("{}-loop{}", index % 4, if index < 4 { 1 } else { 2 });
    }
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
    let hits = [35.0, 62.0, 94.0, 284.0, 2284.0, 2299.0].map(hit);
    let scorer = compare(&expected, &hits, 60.0, 0.0, 30.0, &tempo);
    assert!(
        scorer
            .confirmed
            .iter()
            .any(|event| event.iteration == Some(1))
    );
}

#[test]
fn rejects_late_delivery_and_aborts_on_explicit_work_limit() {
    let expected = notes(20, 0.0);
    let mut scorer = Frontier::new(
        &expected,
        60.0,
        0.0,
        0.0,
        ScoringConfig::default(),
        &[],
        (10, 1000),
    )
    .unwrap();
    assert_eq!(scorer.summary(), Err(Failure::Pending));
    assert_eq!(scorer.push(hit(0.0)), Err(Failure::Overload));
    assert_eq!(scorer.finish(), Err(Failure::Overload));
    assert_eq!(scorer.summary(), Err(Failure::Overload));
    let mut normal = make(&expected);
    normal.advance(1.0).unwrap();
    assert_eq!(normal.push(hit(0.0)), Err(Failure::LateDelivery));
    assert_eq!(normal.finish(), Err(Failure::LateDelivery));
}

#[test]
fn a_violated_watermark_can_invalidate_an_already_emitted_miss() {
    let expected = notes(1, 1000.0);
    let mut scorer = make(&expected);
    scorer.advance(101.0).unwrap();
    assert_eq!(scorer.confirmed[0].judgement, Judgement::Missed);
    // Detection is too late to make the earlier miss safe if a caller did
    // not actually guarantee event-time ordering before advancing.
    assert_eq!(scorer.push(hit(100.0)), Err(Failure::LateDelivery));
    let offline = score_session_with_tempo_map(
        &expected,
        &[hit(100.0)],
        60.0,
        0.0,
        0.0,
        ScoringConfig::default(),
        &[],
    );
    assert_eq!(offline.matched, 1);
    assert_eq!(offline.results[0].judgement, Judgement::Late);
}
