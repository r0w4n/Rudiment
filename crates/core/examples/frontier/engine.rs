//! Isolated candidate only; not production scoring, transport, or presentation.

use rudiment_core::{
    ExpectedHit, Instrument, Judgement, ObservedHit, ScoringConfig, TempoChange, elapsed_ms_at_beat,
};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};

pub const PROPOSED_VERSION: &str = "frontier-v4-proposal";

#[derive(Debug, Clone, PartialEq)]
pub struct Confirmation {
    pub expected_index: Option<usize>,
    pub expected_id: Option<String>,
    pub iteration: Option<usize>,
    pub expected_ms: Option<f64>,
    pub observed_index: Option<usize>,
    pub instrument: Instrument,
    pub error_ms: Option<f64>,
    pub judgement: Judgement,
    pub note_score: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Projection {
    pub observed_index: usize,
    pub expected_index: Option<usize>,
    pub judgement: Judgement,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure {
    InvalidInput,
    LateDelivery,
    Overload,
    Finished,
    Pending,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub scoring_version: &'static str,
    pub score: Option<f64>,
    pub expected: usize,
    pub matched: usize,
    pub missed: usize,
    pub extras: usize,
    pub wrong_instrument: usize,
    pub mean_abs_error_ms: Option<f64>,
    pub signed_mean_error_ms: Option<f64>,
    pub timing_consistency_ms: Option<f64>,
}

struct Budget {
    remaining: usize,
}
impl Budget {
    fn spend(&mut self, amount: usize) -> Result<(), Failure> {
        self.remaining = self
            .remaining
            .checked_sub(amount)
            .ok_or(Failure::Overload)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Default)]
struct Plan {
    count: usize,
    exact: usize,
    error_ns: u128,
    tail: Option<usize>,
}

#[derive(Clone, Copy)]
struct Node {
    note: usize,
    observed: usize,
    previous: Option<usize>,
}

#[derive(Clone)]
struct Target {
    original: usize,
    hit: ExpectedHit,
    ms: f64,
}

struct Partition {
    instrument: Instrument,
    targets: Vec<Target>,
    tree: Vec<Plan>,
    arena: Vec<Node>,
    confirmed_prefix: usize,
    last_observed_ms: f64,
    pending: VecDeque<(usize, usize)>,
    projection: HashMap<usize, Option<usize>>,
}

// Lexicographic pairs are ordered by the per-instrument expected order and the
// globally stable arrival index. Input order is the collision tie-break.
fn pairs(plan: Plan, arena: &[Node], budget: &mut Budget) -> Result<Vec<(usize, usize)>, Failure> {
    budget.spend(plan.count)?;
    let mut result = Vec::with_capacity(plan.count);
    let mut tail = plan.tail;
    while let Some(index) = tail {
        let node = arena[index];
        result.push((node.note, node.observed));
        tail = node.previous;
    }
    result.reverse();
    Ok(result)
}

fn better(a: Plan, b: Plan, arena: &[Node], budget: &mut Budget) -> Result<bool, Failure> {
    budget.spend(1)?;
    let rank = a
        .count
        .cmp(&b.count)
        .then(a.exact.cmp(&b.exact))
        .then_with(|| b.error_ns.cmp(&a.error_ns));
    if rank != Ordering::Equal {
        return Ok(rank == Ordering::Greater);
    }
    if a.tail == b.tail {
        return Ok(false);
    }
    Ok(pairs(a, arena, budget)? < pairs(b, arena, budget)?)
}

impl Partition {
    fn new(instrument: Instrument, mut targets: Vec<Target>) -> Self {
        targets.sort_by(|a, b| a.ms.total_cmp(&b.ms).then(a.original.cmp(&b.original)));
        let tree = vec![Plan::default(); targets.len() + 1];
        Self {
            instrument,
            targets,
            tree,
            arena: vec![],
            confirmed_prefix: 0,
            last_observed_ms: f64::NEG_INFINITY,
            pending: VecDeque::new(),
            projection: HashMap::new(),
        }
    }

    fn query(&self, mut end: usize, budget: &mut Budget) -> Result<Plan, Failure> {
        let mut result = Plan::default();
        while end > 0 {
            if better(self.tree[end], result, &self.arena, budget)? {
                result = self.tree[end];
            }
            end &= end - 1;
        }
        Ok(result)
    }

    fn update(&mut self, note: usize, plan: Plan, budget: &mut Budget) -> Result<(), Failure> {
        let mut index = note + 1;
        while index < self.tree.len() {
            if better(plan, self.tree[index], &self.arena, budget)? {
                self.tree[index] = plan;
            }
            index += index.isolate_lowest_one();
        }
        Ok(())
    }

    fn append(
        &mut self,
        observed_index: usize,
        hit: &ObservedHit,
        time: f64,
        window: f64,
        max_nodes: usize,
        budget: &mut Budget,
    ) -> Result<bool, Failure> {
        self.last_observed_ms = time;
        let first = self
            .targets
            .partition_point(|target| target.ms < (time - window).next_down());
        let end = self
            .targets
            .partition_point(|target| target.ms <= (time + window).next_up());
        if first == end {
            return Ok(false);
        }
        budget.spend(end - first)?;
        let mut updates = Vec::with_capacity(end - first);
        for note in first..end {
            let error = time - self.targets[note].ms;
            // Repeat the authoritative inclusive edge check; binary bounds are
            // only a narrowing operation and may round at floating boundaries.
            if !error.is_finite() || error.abs() > window {
                continue;
            }
            let scaled = (error.abs() * 1_000_000.0).round();
            if !scaled.is_finite() || scaled > u64::MAX as f64 {
                return Err(Failure::InvalidInput);
            }
            let parent = self.query(note, budget)?;
            if self.arena.len() >= max_nodes {
                return Err(Failure::Overload);
            }
            let tail = self.arena.len();
            self.arena.push(Node {
                note,
                observed: observed_index,
                previous: parent.tail,
            });
            updates.push((
                note,
                Plan {
                    count: parent.count + 1,
                    exact: parent.exact
                        + usize::from(hit.raw_note == Some(self.targets[note].hit.note)),
                    error_ns: parent.error_ns + scaled as u128,
                    tail: Some(tail),
                },
            ));
        }
        if updates.is_empty() {
            return Ok(false);
        }
        // Do not update the tree until all candidates for this hit have been
        // evaluated: a single observation may never match two notes.
        for (note, plan) in updates {
            self.update(note, plan, budget)?;
        }
        self.pending.push_back((observed_index, end));
        Ok(true)
    }

    fn assignments(
        &self,
        plan: Plan,
        cut: usize,
        budget: &mut Budget,
    ) -> Result<Vec<Option<usize>>, Failure> {
        budget.spend(cut - self.confirmed_prefix)?;
        let mut assignments = vec![None; cut - self.confirmed_prefix];
        let mut tail = plan.tail;
        while let Some(index) = tail {
            budget.spend(1)?;
            let node = self.arena[index];
            if node.note < self.confirmed_prefix {
                break;
            }
            if node.note < cut {
                assignments[node.note - self.confirmed_prefix] = Some(node.observed);
            }
            tail = node.previous;
        }
        Ok(assignments)
    }

    fn settle(
        &mut self,
        watermark: f64,
        window: f64,
        budget: &mut Budget,
    ) -> Result<Vec<(usize, Option<usize>)>, Failure> {
        // The end boundary is inclusive. A watermark equal to the end does not
        // rule out another valid hit with that exact timestamp.
        let cut = self
            .targets
            .partition_point(|target| watermark - target.ms > window);
        if cut <= self.confirmed_prefix {
            return Ok(vec![]);
        }
        let possible = self
            .targets
            .partition_point(|target| target.ms <= (self.last_observed_ms + window).next_up())
            .max(cut);
        let first = self.query(cut, budget)?;
        let mut common = self.assignments(first, cut, budget)?;
        for frontier in cut + 1..=possible {
            let plan = self.query(frontier, budget)?;
            let candidate = self.assignments(plan, cut, budget)?;
            let agreed = common
                .iter()
                .zip(&candidate)
                .take_while(|(a, b)| a == b)
                .count();
            common.truncate(agreed);
            if common.is_empty() {
                break;
            }
        }
        let start = self.confirmed_prefix;
        self.confirmed_prefix += common.len();
        Ok(common
            .into_iter()
            .enumerate()
            .map(|(index, observed)| (start + index, observed))
            .collect())
    }

    fn project(
        &mut self,
        used: &[bool],
        observed: &[ObservedHit],
        latency: f64,
        config: ScoringConfig,
        budget: &mut Budget,
    ) -> Result<Vec<Projection>, Failure> {
        let best = self.query(self.targets.len(), budget)?;
        let mut assigned = HashMap::new();
        let mut tail = best.tail;
        while let Some(index) = tail {
            budget.spend(1)?;
            let node = self.arena[index];
            if node.note < self.confirmed_prefix {
                break;
            }
            assigned.insert(node.observed, node.note);
            tail = node.previous;
        }
        let mut changes = vec![];
        for &(observed_index, _) in &self.pending {
            budget.spend(1)?;
            if used[observed_index] {
                self.projection.remove(&observed_index);
                continue;
            }
            let note = assigned.get(&observed_index).copied();
            if self.projection.get(&observed_index) == Some(&note) {
                continue;
            }
            self.projection.insert(observed_index, note);
            changes.push(Projection {
                observed_index,
                expected_index: note.map(|index| self.targets[index].original),
                judgement: note
                    .map(|index| {
                        config.classify(
                            observed[observed_index].timestamp_ms
                                - latency
                                - self.targets[index].ms,
                        )
                    })
                    .unwrap_or(Judgement::Extra),
            });
        }
        Ok(changes)
    }
}

pub struct Frontier {
    partitions: Vec<Partition>,
    observed: Vec<ObservedHit>,
    used: Vec<bool>,
    pub confirmed: Vec<Confirmation>,
    latency: f64,
    window: f64,
    config: ScoringConfig,
    watermark: f64,
    last_timestamp: f64,
    max_work: usize,
    max_nodes: usize,
    failure: Option<Failure>,
    finished: bool,
    expected_count: usize,
}

impl Frontier {
    pub fn new(
        expected: &[ExpectedHit],
        bpm: f64,
        start: f64,
        latency: f64,
        config: ScoringConfig,
        tempo: &[TempoChange],
        limits: (usize, usize),
    ) -> Result<Self, Failure> {
        if !bpm.is_finite()
            || bpm <= 0.0
            || !start.is_finite()
            || !latency.is_finite()
            || !config.match_ms.is_finite()
            || !config.good_ms.is_finite()
            || !config.perfect_ms.is_finite()
        {
            return Err(Failure::InvalidInput);
        }
        let window = config
            .match_ms
            .max(config.good_ms)
            .max(config.perfect_ms)
            .max(0.0);
        let mut instruments = Vec::<Instrument>::new();
        let mut ids = HashSet::new();
        for hit in expected {
            if !hit.beat.is_finite() || hit.beat < 0.0 || !ids.insert(&hit.id) {
                return Err(Failure::InvalidInput);
            }
            if !instruments.contains(&hit.instrument) {
                instruments.push(hit.instrument);
            }
        }
        let mut partitions = vec![];
        for instrument in instruments {
            let targets = expected
                .iter()
                .enumerate()
                .filter(|(_, hit)| hit.instrument == instrument)
                .map(|(original, hit)| Target {
                    original,
                    hit: hit.clone(),
                    ms: start + elapsed_ms_at_beat(hit.beat, bpm, tempo),
                })
                .collect::<Vec<_>>();
            if targets.iter().any(|target| !target.ms.is_finite()) {
                return Err(Failure::InvalidInput);
            }
            partitions.push(Partition::new(instrument, targets));
        }
        Ok(Self {
            partitions,
            observed: vec![],
            used: vec![],
            confirmed: vec![],
            latency,
            window,
            config,
            watermark: f64::NEG_INFINITY,
            last_timestamp: f64::NEG_INFINITY,
            max_work: limits.0,
            max_nodes: limits.1,
            failure: None,
            finished: false,
            expected_count: expected.len(),
        })
    }

    fn check(&self) -> Result<(), Failure> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        if self.finished {
            return Err(Failure::Finished);
        }
        Ok(())
    }

    fn invalid_judgement(
        &self,
        hit: &ObservedHit,
        budget: &mut Budget,
    ) -> Result<Judgement, Failure> {
        let time = hit.timestamp_ms - self.latency;
        for partition in &self.partitions {
            budget.spend(1)?;
            if partition.instrument == hit.instrument {
                continue;
            }
            let index = partition.targets.partition_point(|target| target.ms < time);
            let near = [index.checked_sub(1), Some(index)]
                .into_iter()
                .flatten()
                .filter_map(|index| partition.targets.get(index))
                .any(|target| (time - target.ms).abs() <= self.window);
            if near {
                return Ok(Judgement::WrongInstrument);
            }
        }
        Ok(Judgement::Extra)
    }

    fn invalid(&mut self, observed_index: usize, budget: &mut Budget) -> Result<(), Failure> {
        let hit = &self.observed[observed_index];
        let judgement = self.invalid_judgement(hit, budget)?;
        self.confirmed.push(Confirmation {
            expected_index: None,
            expected_id: None,
            iteration: None,
            expected_ms: None,
            observed_index: Some(observed_index),
            instrument: hit.instrument,
            error_ms: None,
            judgement,
            note_score: 0.0,
        });
        Ok(())
    }

    fn advance_inner(&mut self, watermark: f64, budget: &mut Budget) -> Result<(), Failure> {
        if watermark.is_nan() || watermark < self.watermark {
            return Err(Failure::LateDelivery);
        }
        self.watermark = watermark;
        let mut invalid = vec![];
        for partition in &mut self.partitions {
            let settled = partition.settle(watermark, self.window, budget)?;
            for (note, observed_index) in settled {
                let target = &partition.targets[note];
                let error = observed_index
                    .map(|index| self.observed[index].timestamp_ms - self.latency - target.ms);
                if let Some(index) = observed_index {
                    self.used[index] = true;
                    partition.projection.remove(&index);
                }
                let judgement = error
                    .map(|value| self.config.classify(value))
                    .unwrap_or(Judgement::Missed);
                let iteration = target
                    .hit
                    .id
                    .rsplit_once("-loop")
                    .and_then(|(_, suffix)| suffix.parse::<usize>().ok())
                    .and_then(|n| n.checked_sub(1));
                self.confirmed.push(Confirmation {
                    expected_index: Some(target.original),
                    expected_id: Some(target.hit.id.clone()),
                    iteration,
                    expected_ms: Some(target.ms),
                    observed_index,
                    instrument: target.hit.instrument,
                    error_ms: error,
                    judgement,
                    note_score: error
                        .map(|value| self.config.note_score(value))
                        .unwrap_or(0.0),
                });
            }
            while partition
                .pending
                .front()
                .is_some_and(|(_, end)| *end <= partition.confirmed_prefix)
            {
                budget.spend(1)?;
                let (index, _) = partition.pending.pop_front().unwrap();
                partition.projection.remove(&index);
                if !self.used[index] {
                    invalid.push(index);
                }
            }
        }
        for index in invalid {
            self.invalid(index, budget)?;
        }
        Ok(())
    }

    // Callers must provide a monotonic calibrated event-time watermark. A
    // transport frame alone does not establish this promise in the app.
    pub fn advance(&mut self, watermark: f64) -> Result<(), Failure> {
        self.check()?;
        let mut budget = Budget {
            remaining: self.max_work,
        };
        let result = self.advance_inner(watermark, &mut budget);
        if let Err(failure) = result {
            self.failure = Some(failure);
        }
        result
    }

    pub fn push(&mut self, hit: ObservedHit) -> Result<Vec<Projection>, Failure> {
        self.check()?;
        let time = hit.timestamp_ms - self.latency;
        if !hit.timestamp_ms.is_finite() || !time.is_finite() {
            self.failure = Some(Failure::InvalidInput);
            return Err(Failure::InvalidInput);
        }
        if hit.timestamp_ms < self.last_timestamp || time < self.watermark {
            self.failure = Some(Failure::LateDelivery);
            return Err(Failure::LateDelivery);
        }
        self.last_timestamp = hit.timestamp_ms;
        let mut budget = Budget {
            remaining: self.max_work,
        };
        let result = (|| {
            self.advance_inner(time, &mut budget)?;
            let index = self.observed.len();
            self.observed.push(hit.clone());
            self.used.push(false);
            let Some(partition_index) = self
                .partitions
                .iter()
                .position(|partition| partition.instrument == hit.instrument)
            else {
                self.invalid(index, &mut budget)?;
                return Ok(vec![]);
            };
            let matched_candidate = self.partitions[partition_index].append(
                index,
                &hit,
                time,
                self.window,
                self.max_nodes,
                &mut budget,
            )?;
            if !matched_candidate {
                self.invalid(index, &mut budget)?;
                return Ok(vec![]);
            }
            let mut projected = self.partitions[partition_index].project(
                &self.used,
                &self.observed,
                self.latency,
                self.config,
                &mut budget,
            )?;
            for projection in &mut projected {
                if projection.expected_index.is_none() {
                    projection.judgement = self.invalid_judgement(
                        &self.observed[projection.observed_index],
                        &mut budget,
                    )?;
                }
            }
            Ok(projected)
        })();
        if let Err(failure) = result {
            self.failure = Some(failure);
        }
        result
    }

    pub fn finish(&mut self) -> Result<(), Failure> {
        self.advance(f64::INFINITY)?;
        self.finished = true;
        Ok(())
    }

    pub fn summary(&self) -> Result<Summary, Failure> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        if !self.finished {
            return Err(Failure::Pending);
        }
        let mut results: Vec<_> = self
            .confirmed
            .iter()
            .filter(|event| event.expected_index.is_some())
            .collect();
        results.sort_by_key(|event| event.expected_index);
        let errors: Vec<_> = results.iter().filter_map(|event| event.error_ms).collect();
        let matched = errors.len();
        let extras = self
            .confirmed
            .iter()
            .filter(|event| event.judgement == Judgement::Extra)
            .count();
        let wrong_instrument = self
            .confirmed
            .iter()
            .filter(|event| event.judgement == Judgement::WrongInstrument)
            .count();
        let points: f64 = results.iter().map(|event| event.note_score).sum();
        let score = (self.expected_count > 0).then(|| {
            (((points - 50.0 * (extras + wrong_instrument) as f64) / self.expected_count as f64)
                .clamp(0.0, 100.0)
                * 10.0)
                .round()
                / 10.0
        });
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
        Ok(Summary {
            scoring_version: PROPOSED_VERSION,
            score,
            expected: self.expected_count,
            matched,
            missed: self.expected_count - matched,
            extras,
            wrong_instrument,
            mean_abs_error_ms,
            signed_mean_error_ms,
            timing_consistency_ms,
        })
    }

    pub fn nodes(&self) -> usize {
        self.partitions
            .iter()
            .map(|partition| partition.arena.len())
            .sum()
    }
    pub fn pending(&self) -> usize {
        self.partitions
            .iter()
            .map(|partition| partition.pending.len())
            .sum()
    }
}
