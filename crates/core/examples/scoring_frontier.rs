//! Opt-in Stage 2 feasibility experiment. Never linked into the application.
#[path = "frontier/engine.rs"]
mod engine;

#[cfg(any(test, target_arch = "wasm32"))]
use engine::Failure;
use engine::Frontier;
#[cfg(not(target_arch = "wasm32"))]
use rudiment_core::score_session_with_config;
use rudiment_core::{ExpectedHit, InputSource, Instrument, ObservedHit, ScoringConfig, paradiddle};

const MAX_WORK: usize = 100_000;
const MAX_NODES: usize = 1_000_000;

// Diagnostic cumulative allocation requests in the native example only. This
// is not retained/peak memory and is never linked into the application.
#[cfg(not(target_arch = "wasm32"))]
mod allocations {
    use std::alloc::{GlobalAlloc, Layout, System};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    pub struct Counting;
    static ENABLED: AtomicBool = AtomicBool::new(false);
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    static BYTES: AtomicUsize = AtomicUsize::new(0);
    static REALLOCS: AtomicUsize = AtomicUsize::new(0);

    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let result = unsafe { System.alloc(layout) };
            if !result.is_null() && ENABLED.load(Ordering::Relaxed) {
                CALLS.fetch_add(1, Ordering::Relaxed);
                BYTES.fetch_add(layout.size(), Ordering::Relaxed);
            }
            result
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { System.dealloc(ptr, layout) };
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            let result = unsafe { System.realloc(ptr, layout, size) };
            if !result.is_null() && ENABLED.load(Ordering::Relaxed) {
                REALLOCS.fetch_add(1, Ordering::Relaxed);
                BYTES.fetch_add(size, Ordering::Relaxed);
            }
            result
        }
    }
    pub fn start() {
        CALLS.store(0, Ordering::Relaxed);
        BYTES.store(0, Ordering::Relaxed);
        REALLOCS.store(0, Ordering::Relaxed);
        ENABLED.store(true, Ordering::Relaxed);
    }
    pub fn stop() -> (usize, usize, usize) {
        ENABLED.store(false, Ordering::Relaxed);
        (
            CALLS.load(Ordering::Relaxed),
            REALLOCS.load(Ordering::Relaxed),
            BYTES.load(Ordering::Relaxed),
        )
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static ALLOCATOR: allocations::Counting = allocations::Counting;

fn notes(count: usize, spacing: f64) -> Vec<ExpectedHit> {
    let template = paradiddle().hits[0].clone();
    (0..count)
        .map(|i| ExpectedHit {
            id: format!("n{i}-loop{}", i / 16 + 1),
            beat: i as f64 * spacing / 1000.0,
            notation_beat: i as f64 * spacing / 1000.0,
            ..template.clone()
        })
        .collect()
}

fn hit(time: f64) -> ObservedHit {
    ObservedHit {
        timestamp_ms: time,
        instrument: Instrument::Snare,
        velocity: 100,
        source: InputSource::Demo,
        raw_note: None,
        raw_channel: None,
    }
}

fn make(expected: &[ExpectedHit]) -> Frontier {
    Frontier::new(
        expected,
        60.0,
        0.0,
        0.0,
        ScoringConfig::default(),
        &[],
        (MAX_WORK, MAX_NODES),
    )
    .unwrap()
}

#[cfg(not(target_arch = "wasm32"))]
fn percentile(values: &[f64], fraction: f64) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[((sorted.len() - 1) as f64 * fraction).ceil() as usize]
}

#[cfg(not(target_arch = "wasm32"))]
fn run(count: usize, spacing: f64) {
    use std::time::Instant;
    let expected = notes(count, spacing);
    let setup = Instant::now();
    let mut scorer = make(&expected);
    let setup_ms = setup.elapsed().as_secs_f64() * 1000.0;
    let mut samples = Vec::with_capacity(count);
    let mut peak_pending = 0;
    let mut max_confirm_delay = 0.0_f64;
    let mut misses = 0;
    let mut failure = None;
    allocations::start();
    for i in 0..count {
        let timestamp = i as f64 * spacing + 5.0;
        let prior = scorer.confirmed.len();
        let started = Instant::now();
        let result = scorer.push(hit(timestamp));
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
        if let Err(error) = result {
            failure = Some(error);
            break;
        }
        for event in &scorer.confirmed[prior..] {
            if let Some(target) = event.expected_ms {
                max_confirm_delay = max_confirm_delay.max(timestamp - target);
            }
            misses += usize::from(event.judgement == rudiment_core::Judgement::Missed);
        }
        peak_pending = peak_pending.max(scorer.pending());
    }
    let before_finish = scorer
        .confirmed
        .iter()
        .filter(|event| event.expected_index.is_some())
        .count();
    let started = Instant::now();
    if failure.is_none() {
        failure = scorer.finish().err();
    }
    if failure.is_none() {
        std::hint::black_box(scorer.summary().unwrap());
    }
    let finish_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (alloc_calls, realloc_calls, allocated_bytes) = allocations::stop();
    let nodes = scorer.nodes();
    let observed: Vec<_> = (0..count).map(|i| hit(i as f64 * spacing + 5.0)).collect();
    let started = Instant::now();
    let offline_ms = if count <= 2000 {
        std::hint::black_box(score_session_with_config(
            &expected,
            &observed,
            60.0,
            0.0,
            0.0,
            ScoringConfig::default(),
        ));
        Some(started.elapsed().as_secs_f64() * 1000.0)
    } else {
        None
    };
    println!(
        "version={} count={count} spacing_ms={spacing:.2} n={} p50_ms={:.4} p95_ms={:.4} p99_ms={:.4} max_ms={:.4} setup_ms={setup_ms:.3} finish_ms={finish_ms:.3} offline_ms={offline_ms:?} confirmed_before_finish={before_finish} peak_pending={peak_pending} nodes={nodes} max_confirmation_delay_ms={max_confirm_delay:.3} misses={misses} failure={failure:?}",
        engine::PROPOSED_VERSION,
        samples.len(),
        percentile(&samples, 0.50),
        percentile(&samples, 0.95),
        percentile(&samples, 0.99),
        percentile(&samples, 1.0)
    );
    println!(
        "allocations count={count} spacing_ms={spacing:.2} calls={alloc_calls} reallocations={realloc_calls} requested_bytes={allocated_bytes}"
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn run_gap(count: usize, spacing: f64) {
    use std::time::Instant;
    let expected = notes(count, spacing);
    let gap = count / 4;
    let mut scorer = make(&expected);
    let mut samples = vec![];
    let mut failure = None;
    let mut processed = 0;
    let mut peak_pending = 0;
    let mut revisions = 0;
    allocations::start();
    for i in 0..count {
        if i == gap {
            continue;
        }
        let start = Instant::now();
        let result = scorer.push(hit(i as f64 * spacing + 5.0));
        samples.push(start.elapsed().as_secs_f64() * 1000.0);
        match result {
            Ok(projection) => revisions += projection.len(),
            Err(error) => {
                failure = Some(error);
                break;
            }
        }
        processed += 1;
        peak_pending = peak_pending.max(scorer.pending());
    }
    let before_finish = scorer
        .confirmed
        .iter()
        .filter(|event| event.expected_index.is_some())
        .count();
    let miss_before_finish = scorer.confirmed.iter().any(|event| {
        event.expected_index == Some(gap) && event.judgement == rudiment_core::Judgement::Missed
    });
    let started = Instant::now();
    if failure.is_none() {
        failure = scorer.finish().err();
    }
    if failure.is_none() {
        std::hint::black_box(scorer.summary().unwrap());
    }
    let finish_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (alloc_calls, realloc_calls, allocated_bytes) = allocations::stop();
    let observed: Vec<_> = (0..count)
        .filter(|i| *i != gap)
        .map(|i| hit(i as f64 * spacing + 5.0))
        .collect();
    let started = Instant::now();
    let offline_ms = if count <= 2000 {
        std::hint::black_box(score_session_with_config(
            &expected,
            &observed,
            60.0,
            0.0,
            0.0,
            ScoringConfig::default(),
        ));
        Some(started.elapsed().as_secs_f64() * 1000.0)
    } else {
        None
    };
    println!(
        "gap count={count} spacing_ms={spacing:.2} gap_index={gap} processed={processed} p50_ms={:.4} p95_ms={:.4} p99_ms={:.4} max_ms={:.4} confirmed_before_finish={before_finish} missing_confirmed_before_finish={miss_before_finish} peak_pending={peak_pending} nodes={} projections={revisions} finish_ms={finish_ms:.3} offline_ms={offline_ms:?} failure={failure:?}",
        percentile(&samples, 0.50),
        percentile(&samples, 0.95),
        percentile(&samples, 0.99),
        percentile(&samples, 1.0),
        scorer.nodes()
    );
    println!(
        "gap_allocations count={count} spacing_ms={spacing:.2} calls={alloc_calls} reallocations={realloc_calls} requested_bytes={allocated_bytes}"
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn run_frames(count: usize, spacing: f64, gap: bool) {
    use std::time::Instant;
    let expected = notes(count, spacing);
    let gap_index = count / 4;
    let mut scorer = make(&expected);
    let mut frame_time = 0.0;
    let mut frame_samples = vec![];
    let mut earliest_miss = None;
    let mut max_delay = 0.0_f64;
    let mut peak_pending = 0;
    let mut failure = None;
    let mut checked = 0;
    let record = |scorer: &Frontier,
                  now: f64,
                  checked: &mut usize,
                  earliest_miss: &mut Option<f64>,
                  max_delay: &mut f64| {
        for event in &scorer.confirmed[*checked..] {
            if let Some(target) = event.expected_ms {
                *max_delay = max_delay.max(now - target);
                if event.expected_index == Some(gap_index)
                    && event.judgement == rudiment_core::Judgement::Missed
                {
                    *earliest_miss = Some(now - target);
                }
            }
        }
        *checked = scorer.confirmed.len();
    };
    for i in 0..count {
        let timestamp = i as f64 * spacing + 5.0;
        while frame_time < timestamp {
            let start = Instant::now();
            let result = scorer.advance(frame_time);
            frame_samples.push(start.elapsed().as_secs_f64() * 1000.0);
            if let Err(error) = result {
                failure = Some(error);
                break;
            }
            record(
                &scorer,
                frame_time,
                &mut checked,
                &mut earliest_miss,
                &mut max_delay,
            );
            peak_pending = peak_pending.max(scorer.pending());
            frame_time += 1000.0 / 60.0;
        }
        if failure.is_some() {
            break;
        }
        if !gap || i != gap_index {
            if let Err(error) = scorer.push(hit(timestamp)) {
                failure = Some(error);
                break;
            }
            record(
                &scorer,
                timestamp,
                &mut checked,
                &mut earliest_miss,
                &mut max_delay,
            );
        }
    }
    let before_end = scorer
        .confirmed
        .iter()
        .filter(|event| event.expected_index.is_some())
        .count();
    let miss_before_end = earliest_miss;
    let end = (count - 1) as f64 * spacing + 120.0;
    while failure.is_none() && frame_time <= end {
        let start = Instant::now();
        let result = scorer.advance(frame_time);
        frame_samples.push(start.elapsed().as_secs_f64() * 1000.0);
        if let Err(error) = result {
            failure = Some(error);
            break;
        }
        record(
            &scorer,
            frame_time,
            &mut checked,
            &mut earliest_miss,
            &mut max_delay,
        );
        frame_time += 1000.0 / 60.0;
    }
    println!(
        "frames count={count} spacing_ms={spacing:.2} gap={gap} samples={} frame_p50_ms={:.4} frame_p95_ms={:.4} frame_p99_ms={:.4} frame_max_ms={:.4} confirmed_before_end={before_end} miss_delay_before_end_ms={miss_before_end:?} miss_delay_after_drain_ms={earliest_miss:?} max_confirmation_delay_ms={max_delay:.3} peak_pending={peak_pending} nodes={} failure={failure:?}",
        frame_samples.len(),
        percentile(&frame_samples, 0.50),
        percentile(&frame_samples, 0.95),
        percentile(&frame_samples, 0.99),
        percentile(&frame_samples, 1.0),
        scorer.nodes()
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn run_midi(filename: &str) {
    use std::time::Instant;
    let bytes = std::fs::read(format!("web/public/exercises/rudiments/{filename}"))
        .expect("run from repository root with built-in fixtures present");
    let exercise = rudiment_core::parse_midi(&bytes, filename, None).unwrap();
    let mut observed: Vec<_> = exercise
        .hits
        .iter()
        .map(|note| ObservedHit {
            timestamp_ms: rudiment_core::elapsed_ms_at_beat(
                note.beat,
                exercise.tempo_bpm,
                &exercise.tempo_map,
            ) + 5.0,
            instrument: note.instrument,
            velocity: 100,
            source: InputSource::Midi,
            raw_note: Some(note.note),
            raw_channel: Some(note.channel),
        })
        .collect();
    observed.sort_by(|left, right| left.timestamp_ms.total_cmp(&right.timestamp_ms));
    let mut scorer = Frontier::new(
        &exercise.hits,
        exercise.tempo_bpm,
        0.0,
        0.0,
        ScoringConfig::default(),
        &exercise.tempo_map,
        (MAX_WORK, MAX_NODES),
    )
    .unwrap();
    let mut samples = Vec::with_capacity(observed.len());
    let mut failure = None;
    allocations::start();
    for event in &observed {
        let started = Instant::now();
        let result = scorer.push(event.clone());
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
        if let Err(error) = result {
            failure = Some(error);
            break;
        }
    }
    let confirmed_before_finish = scorer
        .confirmed
        .iter()
        .filter(|event| event.expected_index.is_some())
        .count();
    let pending = scorer.pending();
    let started = Instant::now();
    if failure.is_none() {
        failure = scorer.finish().err();
    }
    let summary = if failure.is_none() {
        Some(scorer.summary().unwrap())
    } else {
        None
    };
    let finish_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (alloc_calls, realloc_calls, allocated_bytes) = allocations::stop();
    let started = Instant::now();
    let legacy = rudiment_core::score_session_v3_with_tempo_map(
        &exercise.hits,
        &observed,
        exercise.tempo_bpm,
        0.0,
        0.0,
        ScoringConfig::default(),
        &exercise.tempo_map,
    );
    let offline_ms = started.elapsed().as_secs_f64() * 1000.0;
    println!(
        "midi={filename} notes={} grace={} p50_ms={:.4} p95_ms={:.4} p99_ms={:.4} max_ms={:.4} confirmed_before_finish={confirmed_before_finish} pending={pending} nodes={} finish_ms={finish_ms:.3} offline_ms={offline_ms:.3} score={:?} v3_score={:?} alloc_calls={alloc_calls} realloc_calls={realloc_calls} allocated_bytes={allocated_bytes} failure={failure:?}",
        exercise.hits.len(),
        exercise.hits.iter().filter(|hit| hit.grace).count(),
        percentile(&samples, 0.50),
        percentile(&samples, 0.95),
        percentile(&samples, 0.99),
        percentile(&samples, 1.0),
        scorer.nodes(),
        summary.and_then(|value| value.score),
        legacy.score
    );
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() == 3 && args[2] == "gap" {
        run_gap(args[0].parse().unwrap(), args[1].parse().unwrap());
        return;
    }
    if args.len() == 2 {
        run(args[0].parse().unwrap(), args[1].parse().unwrap());
        return;
    }
    for (count, spacing) in [
        (500, 1000.0),
        (500, 250.0),
        (500, 125.0),
        (500, 62.5),
        (1000, 31.25),
        (2000, 62.5),
        (10000, 62.5),
        (500, 0.0),
    ] {
        run(count, spacing);
    }
    run_gap(2000, 62.5);
    run_frames(500, 250.0, false);
    run_frames(500, 62.5, true);
    run_frames(2000, 62.5, true);
    for filename in [
        "12_beginner_single_strokes.mid",
        "06_single_paradiddle.mid",
        "05_hard_flams.mid",
        "08_flam_paradiddle.mid",
        "09_single_drag.mid",
    ] {
        run_midi(filename);
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {}

// Raw, opt-in WASM exports are exercised only by the isolated benchmark. The
// production bridge does not import these functions or this WASM module.
#[cfg(target_arch = "wasm32")]
thread_local! {
    static WASM_SCORER: std::cell::RefCell<Option<Frontier>> = const { std::cell::RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn frontier_setup(count: usize, spacing: f64) {
    WASM_SCORER.with(|state| *state.borrow_mut() = Some(make(&notes(count, spacing))));
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn frontier_hit(index: usize, spacing: f64) -> i32 {
    WASM_SCORER.with(|state| {
        match state
            .borrow_mut()
            .as_mut()
            .unwrap()
            .push(hit(index as f64 * spacing + 5.0))
        {
            Ok(_) => 0,
            Err(Failure::Overload) => 1,
            Err(_) => 2,
        }
    })
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn frontier_advance(time: f64) -> i32 {
    WASM_SCORER.with(
        |state| match state.borrow_mut().as_mut().unwrap().advance(time) {
            Ok(()) => 0,
            Err(Failure::Overload) => 1,
            Err(_) => 2,
        },
    )
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn frontier_finish() -> i32 {
    WASM_SCORER.with(|state| {
        let mut scorer = state.borrow_mut();
        let scorer = scorer.as_mut().unwrap();
        match scorer.finish().and_then(|()| scorer.summary().map(|_| ())) {
            Ok(()) => 0,
            Err(_) => 1,
        }
    })
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn frontier_confirmed() -> usize {
    WASM_SCORER.with(|state| state.borrow().as_ref().unwrap().confirmed.len())
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn frontier_nodes() -> usize {
    WASM_SCORER.with(|state| state.borrow().as_ref().unwrap().nodes())
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn frontier_pending() -> usize {
    WASM_SCORER.with(|state| state.borrow().as_ref().unwrap().pending())
}

#[cfg(target_arch = "wasm32")]
#[unsafe(no_mangle)]
pub extern "C" fn frontier_misses() -> usize {
    WASM_SCORER.with(|state| {
        state
            .borrow()
            .as_ref()
            .unwrap()
            .confirmed
            .iter()
            .filter(|event| event.judgement == rudiment_core::Judgement::Missed)
            .count()
    })
}

#[cfg(test)]
#[path = "frontier/tests.rs"]
mod tests;
