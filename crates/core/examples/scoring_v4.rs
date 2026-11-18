//! Opt-in native timing diagnostic for the production v4 incremental scorer.

use rudiment_core::{
    ExpectedHit, IncrementalScorer, InputSource, Instrument, ObservedHit, ScoringConfig, paradiddle,
};
use std::time::Instant;

fn percentile(samples: &[f64], fraction: f64) -> f64 {
    samples[((samples.len() - 1) as f64 * fraction).ceil() as usize]
}

fn run(count: usize, spacing_ms: f64, gap: bool) {
    let template = paradiddle().hits[0].clone();
    let expected = (0..count)
        .map(|index| ExpectedHit {
            id: format!("v4-{index}"),
            tick: index as u64,
            beat: index as f64 * spacing_ms / 1_000.0,
            notation_beat: index as f64 * spacing_ms / 1_000.0,
            ..template.clone()
        })
        .collect::<Vec<_>>();
    let mut scorer = IncrementalScorer::new(&expected, 60.0, 0.0, ScoringConfig::default(), &[]);
    let mut samples = Vec::with_capacity(count);
    for index in 0..count {
        if gap && index == count / 4 {
            continue;
        }
        let timestamp_ms = index as f64 * spacing_ms + 5.0;
        let hit = ObservedHit {
            timestamp_ms,
            instrument: Instrument::Snare,
            articulation: Default::default(),
            velocity: 100,
            source: InputSource::Demo,
            raw_note: Some(38),
            raw_channel: Some(9),
        };
        let start = Instant::now();
        scorer
            .advance_event_time((timestamp_ms - 32.0).max(0.0))
            .unwrap();
        scorer.observe(hit, timestamp_ms).unwrap();
        std::hint::black_box(scorer.take_feedback_update().unwrap());
        samples.push(start.elapsed().as_secs_f64() * 1_000.0);
    }
    samples.sort_by(f64::total_cmp);
    let finish = Instant::now();
    let summary = scorer.finish().unwrap();
    let finish_ms = finish.elapsed().as_secs_f64() * 1_000.0;
    println!(
        "v4 count={count} spacing_ms={spacing_ms:.2} gap={gap} p50_ms={:.4} p95_ms={:.4} p99_ms={:.4} max_ms={:.4} finish_ms={finish_ms:.4} matched={} missed={} extras={}",
        percentile(&samples, 0.50),
        percentile(&samples, 0.95),
        percentile(&samples, 0.99),
        samples.last().copied().unwrap_or_default(),
        summary.matched,
        summary.missed,
        summary.extras,
    );
}

fn main() {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    if arguments.len() >= 2 {
        run(
            arguments[0].parse().expect("count"),
            arguments[1].parse().expect("spacing milliseconds"),
            arguments.get(2).is_some_and(|value| value == "gap"),
        );
        return;
    }
    for (count, spacing_ms, gap) in [
        (500, 250.0, false),
        (500, 62.5, false),
        (2_000, 62.5, true),
        (10_000, 62.5, true),
    ] {
        run(count, spacing_ms, gap);
    }
}
