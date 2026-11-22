//! Deterministic adaptive-learning decision controller.
//!
//! This module deliberately contains no playback or browser concerns. It
//! consumes completed attempt summaries plus the current practice context and
//! returns the next tempo, bar selection and a human-readable explanation. The
//! web application remains responsible for transport orchestration.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::{Judgement, SessionSummary, performance_rating};

/// Default percentage of the original tempo used for the first recovery step.
pub const DEFAULT_INITIAL_RECOVERY_PERCENT: f64 = 50.0;
pub const DEFAULT_MIN_BPM: f64 = 50.0;
pub const DEFAULT_TEMPO_INCREASE_BPM: f64 = 5.0;
pub const DEFAULT_SECONDARY_REDUCTION_PERCENT: f64 = 10.0;
pub const DEFAULT_FAILED_ATTEMPTS_BEFORE_SLOW: u32 = 2;
pub const DEFAULT_SUCCESSFUL_REPETITIONS_REQUIRED: u32 = 2;
pub const DEFAULT_NEIGHBOUR_BARS: usize = 1;
pub const DEFAULT_FULL_SPEED_CHECK_INTERVAL: u32 = 4;

/// User-configurable adaptive-learning behaviour.
///
/// Values are clamped by [`AdaptiveSettings::normalized`] before the controller
/// uses them. Keeping the raw values in settings makes the UI forgiving while
/// preserving deterministic decisions.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdaptiveSettings {
    /// Initial recovery tempo as a percentage of the original target tempo.
    pub initial_recovery_percent: f64,
    pub min_bpm: f64,
    pub tempo_increase_bpm: f64,
    pub secondary_reduction_percent: f64,
    pub failed_attempts_before_slow: u32,
    pub successful_repetitions_required: u32,
    pub neighbour_bars: usize,
    pub full_speed_checks: bool,
    pub full_speed_check_interval: u32,
}

impl Default for AdaptiveSettings {
    fn default() -> Self {
        Self {
            initial_recovery_percent: DEFAULT_INITIAL_RECOVERY_PERCENT,
            min_bpm: DEFAULT_MIN_BPM,
            tempo_increase_bpm: DEFAULT_TEMPO_INCREASE_BPM,
            secondary_reduction_percent: DEFAULT_SECONDARY_REDUCTION_PERCENT,
            failed_attempts_before_slow: DEFAULT_FAILED_ATTEMPTS_BEFORE_SLOW,
            successful_repetitions_required: DEFAULT_SUCCESSFUL_REPETITIONS_REQUIRED,
            neighbour_bars: DEFAULT_NEIGHBOUR_BARS,
            full_speed_checks: true,
            full_speed_check_interval: DEFAULT_FULL_SPEED_CHECK_INTERVAL,
        }
    }
}

impl AdaptiveSettings {
    /// Clamp every field to its documented range.
    pub fn normalized(self) -> Self {
        Self {
            initial_recovery_percent: self.initial_recovery_percent.clamp(30.0, 90.0),
            min_bpm: self.min_bpm.clamp(30.0, 100.0),
            tempo_increase_bpm: self.tempo_increase_bpm.clamp(1.0, 20.0),
            secondary_reduction_percent: self.secondary_reduction_percent.clamp(5.0, 30.0),
            failed_attempts_before_slow: self.failed_attempts_before_slow.clamp(1, 5),
            successful_repetitions_required: self.successful_repetitions_required.clamp(1, 5),
            neighbour_bars: self.neighbour_bars.min(2),
            full_speed_checks: self.full_speed_checks,
            full_speed_check_interval: self.full_speed_check_interval.clamp(1, 10),
        }
    }
}

/// High-level adaptive session state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdaptivePhase {
    Inactive,
    InitialAssessment,
    Recovery,
    FocusedPractice,
    FullExercisePractice,
    TempoProbe,
    Completed,
}

impl AdaptivePhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Inactive => "Inactive",
            Self::InitialAssessment => "Initial assessment",
            Self::Recovery => "Recovery",
            Self::FocusedPractice => "Focused practice",
            Self::FullExercisePractice => "Full exercise",
            Self::TempoProbe => "Full-speed check",
            Self::Completed => "Completed",
        }
    }
}

/// The concrete action the playback layer should take at an attempt boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdaptiveAction {
    Start,
    Repeat,
    ReduceTempo,
    IncreaseTempo,
    FocusBar,
    ReturnToFullExercise,
    FullSpeedProbe,
    ReturnToRecoveryTempo,
    Complete,
}

impl AdaptiveAction {
    pub fn label(self) -> &'static str {
        match self {
            Self::Start => "Start",
            Self::Repeat => "Repeat",
            Self::ReduceTempo => "Slow down",
            Self::IncreaseTempo => "Speed up",
            Self::FocusBar => "Focus on difficult bars",
            Self::ReturnToFullExercise => "Return to full exercise",
            Self::FullSpeedProbe => "Full-speed check",
            Self::ReturnToRecoveryTempo => "Return to recovery tempo",
            Self::Complete => "Complete",
        }
    }
}

/// A completed attempt presented to the controller.
#[derive(Debug, Clone)]
pub struct AdaptiveAttempt {
    pub summary: SessionSummary,
    /// The concrete bars that were played for this attempt, in score order.
    pub performed_bars: Vec<usize>,
    /// Tempo used for this attempt. Normally equals the controller tempo, but
    /// the explicit value keeps the controller resilient to external changes.
    pub current_bpm: f64,
}

/// The next adaptive step. Playback applies this only at an attempt boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct AdaptiveDecision {
    pub phase: AdaptivePhase,
    pub action: AdaptiveAction,
    pub bpm: f64,
    pub bars: Vec<usize>,
    pub target_bpm: f64,
    pub stars: u8,
    pub explanation: String,
    pub completed: bool,
}

/// Deterministic adaptive-learning state machine.
#[derive(Debug, Clone)]
pub struct AdaptiveController {
    settings: AdaptiveSettings,
    phase: AdaptivePhase,
    target_bpm: f64,
    min_bpm: f64,
    current_bpm: f64,
    original_bars: Vec<usize>,
    total_bars: usize,
    expected_per_bar: Vec<usize>,
    bar_beats: f64,
    focus_bars: Option<Vec<usize>>,
    focus_problem_bar: Option<usize>,
    successful_focus_streak: u32,
    failed_streak: u32,
    tempo_increases_since_probe: u32,
    probe_return_bpm: Option<f64>,
    last_stars: u8,
}

impl AdaptiveController {
    /// Build a controller for the original active exercise selection.
    ///
    /// `expected_per_bar` must be indexed by source bar and contain the number
    /// of expected notes in each bar. `original_bars` is the user's active bar
    /// selection (already expanded to concrete bar indices).
    pub fn new(
        settings: AdaptiveSettings,
        target_bpm: f64,
        original_bars: Vec<usize>,
        total_bars: usize,
        expected_per_bar: Vec<usize>,
        bar_beats: f64,
    ) -> Self {
        let settings = settings.normalized();
        let target_bpm = target_bpm.max(1.0);
        let total_bars = total_bars.max(1);
        let mut original_bars = original_bars
            .into_iter()
            .filter(|bar| *bar < total_bars)
            .collect::<Vec<_>>();
        original_bars.sort_unstable();
        original_bars.dedup();
        if original_bars.is_empty() {
            original_bars = (0..total_bars).collect();
        }
        let mut expected_per_bar = expected_per_bar;
        expected_per_bar.resize(total_bars, 0);
        Self {
            min_bpm: settings.min_bpm.min(target_bpm),
            settings,
            phase: AdaptivePhase::Inactive,
            target_bpm,
            current_bpm: target_bpm,
            original_bars,
            total_bars,
            expected_per_bar,
            bar_beats: bar_beats.max(0.000_001),
            focus_bars: None,
            focus_problem_bar: None,
            successful_focus_streak: 0,
            failed_streak: 0,
            tempo_increases_since_probe: 0,
            probe_return_bpm: None,
            last_stars: 0,
        }
    }

    pub fn phase(&self) -> AdaptivePhase {
        self.phase
    }

    pub fn current_bpm(&self) -> f64 {
        self.current_bpm
    }

    pub fn target_bpm(&self) -> f64 {
        self.target_bpm
    }

    pub fn original_bars(&self) -> &[usize] {
        &self.original_bars
    }

    pub fn focus_bars(&self) -> Option<&[usize]> {
        self.focus_bars.as_deref()
    }

    pub fn settings(&self) -> AdaptiveSettings {
        self.settings
    }

    /// Begin a fresh adaptive session at the original target tempo.
    pub fn start(&mut self) -> AdaptiveDecision {
        self.phase = AdaptivePhase::InitialAssessment;
        self.current_bpm = self.target_bpm;
        self.focus_bars = None;
        self.focus_problem_bar = None;
        self.successful_focus_streak = 0;
        self.failed_streak = 0;
        self.tempo_increases_since_probe = 0;
        self.probe_return_bpm = None;
        self.decision(
            AdaptiveAction::Start,
            self.target_bpm,
            self.original_bars.clone(),
            0,
            format!(
                "Start at the original {} BPM and assess the full selection.",
                format_bpm(self.target_bpm)
            ),
            false,
        )
    }

    /// Reset to the inactive state. The next [`start`](Self::start) begins a
    /// fresh session at the original target tempo.
    pub fn reset(&mut self) {
        self.phase = AdaptivePhase::Inactive;
        self.current_bpm = self.target_bpm;
        self.focus_bars = None;
        self.focus_problem_bar = None;
        self.successful_focus_streak = 0;
        self.failed_streak = 0;
        self.tempo_increases_since_probe = 0;
        self.probe_return_bpm = None;
        self.last_stars = 0;
    }

    /// Consume one completed attempt and return the next adaptive step.
    pub fn observe(&mut self, attempt: &AdaptiveAttempt) -> AdaptiveDecision {
        let stars = performance_rating(&attempt.summary).stars;
        self.last_stars = stars;
        let performed = normalize_bars(&attempt.performed_bars);
        let current = attempt.current_bpm.max(1.0);
        match self.phase {
            AdaptivePhase::Inactive => self.start(),
            AdaptivePhase::InitialAssessment => {
                self.on_initial(stars, &performed, current, &attempt.summary)
            }
            AdaptivePhase::Recovery => {
                self.on_recovery(stars, &performed, current, &attempt.summary)
            }
            AdaptivePhase::FocusedPractice => {
                self.on_focused(stars, &performed, current, &attempt.summary)
            }
            AdaptivePhase::FullExercisePractice => {
                self.on_full_exercise(stars, &performed, current, &attempt.summary)
            }
            AdaptivePhase::TempoProbe => self.on_probe(stars, &performed, current),
            AdaptivePhase::Completed => self.decision(
                AdaptiveAction::Complete,
                self.current_bpm,
                self.original_bars.clone(),
                stars,
                "Adaptive learning is already complete.".to_string(),
                true,
            ),
        }
    }

    fn on_initial(
        &mut self,
        stars: u8,
        performed: &[usize],
        current: f64,
        summary: &SessionSummary,
    ) -> AdaptiveDecision {
        if stars >= 3 && self.is_full(performed) && self.at_target(current) {
            return self.complete(
                stars,
                "Three stars at the original tempo. Adaptive learning complete.",
            );
        }
        if stars >= 2 {
            self.current_bpm = current;
            let explanation = if stars == 3 {
                format!(
                    "Three stars, but the full exercise is not yet at {} BPM. Repeat at the current tempo.",
                    format_bpm(self.target_bpm)
                )
            } else {
                format!(
                    "Two stars — repeat the full selection at {} BPM before changing tempo.",
                    format_bpm(current)
                )
            };
            return self.decision(
                AdaptiveAction::Repeat,
                current,
                performed.to_vec(),
                stars,
                explanation,
                false,
            );
        }

        let recovered = self.initial_recovery_bpm();
        self.current_bpm = recovered;
        self.failed_streak = 0;
        self.successful_focus_streak = 0;
        self.tempo_increases_since_probe = 0;
        self.probe_return_bpm = None;

        // The initial assessment always starts with an aggressive reduction.
        // If the errors are concentrated, the same boundary also narrows the
        // selection to the difficult bar and its neighbours.
        if let Some(bar) = self.detect_problem_bar(performed, summary) {
            self.set_focus(bar);
            self.phase = AdaptivePhase::FocusedPractice;
            let bars = self.focus_bars.clone().unwrap_or_default();
            return self.decision(
                AdaptiveAction::FocusBar,
                recovered,
                bars.clone(),
                stars,
                format!(
                    "{} star{} — reduce to {} BPM ({}% of the original tempo) and practise bars {}, with extra attention on bar {}.",
                    stars,
                    if stars == 1 { "" } else { "s" },
                    format_bpm(recovered),
                    format_percent(self.settings.initial_recovery_percent),
                    format_bars(&bars),
                    bar + 1
                ),
                false,
            );
        }

        self.phase = AdaptivePhase::Recovery;
        self.focus_bars = None;
        self.focus_problem_bar = None;
        self.decision(
            AdaptiveAction::ReduceTempo,
            recovered,
            self.original_bars.clone(),
            stars,
            format!(
                "{} star{} — reduce to {} BPM ({}% of the original tempo). Errors are spread out, so keep the broader selection.",
                stars,
                if stars == 1 { "" } else { "s" },
                format_bpm(recovered),
                format_percent(self.settings.initial_recovery_percent)
            ),
            false,
        )
    }

    fn on_recovery(
        &mut self,
        stars: u8,
        performed: &[usize],
        current: f64,
        summary: &SessionSummary,
    ) -> AdaptiveDecision {
        if stars >= 3 {
            self.failed_streak = 0;
            if self.is_full(performed) && self.at_target(current) {
                return self.complete(
                    stars,
                    "Three stars at the original tempo. Adaptive learning complete.",
                );
            }
            let next = self.increase(current);
            self.current_bpm = next;
            if next > current + 0.000_001 {
                self.tempo_increases_since_probe += 1;
            }
            if self.probe_due(next) {
                return self.begin_probe(next, stars);
            }
            self.phase = AdaptivePhase::FullExercisePractice;
            return self.decision(
                AdaptiveAction::IncreaseTempo,
                next,
                self.original_bars.clone(),
                stars,
                format!(
                    "Three stars — increase to {} BPM and continue the full selection.",
                    format_bpm(next)
                ),
                false,
            );
        }
        if stars == 2 {
            self.failed_streak = 0;
            self.current_bpm = current;
            return self.decision(
                AdaptiveAction::Repeat,
                current,
                self.original_bars.clone(),
                stars,
                format!(
                    "Two stars — repeat at {} BPM before increasing speed.",
                    format_bpm(current)
                ),
                false,
            );
        }

        self.failed_streak += 1;
        if self.failed_streak < self.settings.failed_attempts_before_slow {
            self.current_bpm = current;
            return self.decision(
                AdaptiveAction::Repeat,
                current,
                self.original_bars.clone(),
                stars,
                format!(
                    "Attempt {} of {} before slowing at {} BPM.",
                    self.failed_streak,
                    self.settings.failed_attempts_before_slow,
                    format_bpm(current)
                ),
                false,
            );
        }

        self.failed_streak = 0;
        let reduced = self.reduce(current);
        self.current_bpm = reduced;
        if let Some(bar) = self.detect_problem_bar(performed, summary) {
            self.set_focus(bar);
            self.phase = AdaptivePhase::FocusedPractice;
            let bars = self.focus_bars.clone().unwrap_or_default();
            return self.decision(
                AdaptiveAction::FocusBar,
                reduced,
                bars.clone(),
                stars,
                format!(
                    "Concentrated errors in bar {}. Slow to {} BPM and practise bars {}.",
                    bar + 1,
                    format_bpm(reduced),
                    format_bars(&bars)
                ),
                false,
            );
        }
        self.phase = AdaptivePhase::Recovery;
        self.decision(
            AdaptiveAction::ReduceTempo,
            reduced,
            self.original_bars.clone(),
            stars,
            format!(
                "Errors are spread across the exercise. Reduce to {} BPM and repeat the broader selection.",
                format_bpm(reduced)
            ),
            false,
        )
    }

    fn on_focused(
        &mut self,
        stars: u8,
        performed: &[usize],
        current: f64,
        summary: &SessionSummary,
    ) -> AdaptiveDecision {
        let focus = self
            .focus_bars
            .clone()
            .unwrap_or_else(|| performed.to_vec());
        if stars >= 3 {
            self.failed_streak = 0;
            self.successful_focus_streak += 1;
            let next = self.increase(current);
            self.current_bpm = next;
            if next > current + 0.000_001 {
                self.tempo_increases_since_probe += 1;
            }
            if self.successful_focus_streak >= self.settings.successful_repetitions_required {
                self.successful_focus_streak = 0;
                self.phase = AdaptivePhase::FullExercisePractice;
                self.focus_bars = None;
                if self.probe_due(next) {
                    return self.begin_probe(next, stars);
                }
                return self.decision(
                    AdaptiveAction::ReturnToFullExercise,
                    next,
                    self.original_bars.clone(),
                    stars,
                    format!(
                        "Focused passage reliable at {} BPM. Restore the full selection.",
                        format_bpm(next)
                    ),
                    false,
                );
            }
            if self.probe_due(next) {
                return self.begin_probe(next, stars);
            }
            let remaining = self
                .settings
                .successful_repetitions_required
                .saturating_sub(self.successful_focus_streak);
            return self.decision(
                AdaptiveAction::IncreaseTempo,
                next,
                focus,
                stars,
                format!(
                    "Focused passage succeeded at {} BPM. {} more clean repetition{} before returning to the full selection.",
                    format_bpm(next),
                    remaining,
                    if remaining == 1 { "" } else { "s" }
                ),
                false,
            );
        }
        if stars == 2 {
            self.successful_focus_streak = 0;
            self.failed_streak = 0;
            self.current_bpm = current;
            return self.decision(
                AdaptiveAction::Repeat,
                current,
                focus,
                stars,
                format!(
                    "Two stars on the focused passage — repeat at {} BPM.",
                    format_bpm(current)
                ),
                false,
            );
        }

        self.successful_focus_streak = 0;
        self.failed_streak += 1;
        if self.failed_streak < self.settings.failed_attempts_before_slow {
            self.current_bpm = current;
            return self.decision(
                AdaptiveAction::Repeat,
                current,
                focus,
                stars,
                format!(
                    "Attempt {} of {} before slowing the focused passage at {} BPM.",
                    self.failed_streak,
                    self.settings.failed_attempts_before_slow,
                    format_bpm(current)
                ),
                false,
            );
        }

        self.failed_streak = 0;
        let reduced = self.reduce(current);
        self.current_bpm = reduced;
        if let Some(bar) = self.detect_problem_bar(performed, summary) {
            self.set_focus(bar);
        }
        let bars = self.focus_bars.clone().unwrap_or(focus);
        self.phase = AdaptivePhase::FocusedPractice;
        self.decision(
            AdaptiveAction::ReduceTempo,
            reduced,
            bars.clone(),
            stars,
            format!(
                "Focused passage still unstable. Slow to {} BPM and repeat bars {}.",
                format_bpm(reduced),
                format_bars(&bars)
            ),
            false,
        )
    }

    fn on_full_exercise(
        &mut self,
        stars: u8,
        performed: &[usize],
        current: f64,
        summary: &SessionSummary,
    ) -> AdaptiveDecision {
        if stars >= 3 {
            self.failed_streak = 0;
            if self.is_full(performed) && self.at_target(current) {
                return self.complete(
                    stars,
                    "Three stars on the full exercise at the original tempo. Adaptive learning complete.",
                );
            }
            let next = self.increase(current);
            self.current_bpm = next;
            if next > current + 0.000_001 {
                self.tempo_increases_since_probe += 1;
            }
            if self.probe_due(next) {
                return self.begin_probe(next, stars);
            }
            return self.decision(
                AdaptiveAction::IncreaseTempo,
                next,
                self.original_bars.clone(),
                stars,
                format!(
                    "Full selection succeeded — increase to {} BPM.",
                    format_bpm(next)
                ),
                false,
            );
        }
        if stars == 2 {
            self.failed_streak = 0;
            self.current_bpm = current;
            return self.decision(
                AdaptiveAction::Repeat,
                current,
                self.original_bars.clone(),
                stars,
                format!(
                    "Two stars on the full selection — repeat at {} BPM.",
                    format_bpm(current)
                ),
                false,
            );
        }

        self.failed_streak += 1;
        if self.failed_streak < self.settings.failed_attempts_before_slow {
            self.current_bpm = current;
            return self.decision(
                AdaptiveAction::Repeat,
                current,
                self.original_bars.clone(),
                stars,
                format!(
                    "Attempt {} of {} before slowing the full selection at {} BPM.",
                    self.failed_streak,
                    self.settings.failed_attempts_before_slow,
                    format_bpm(current)
                ),
                false,
            );
        }

        self.failed_streak = 0;
        let reduced = self.reduce(current);
        self.current_bpm = reduced;
        if let Some(bar) = self.detect_problem_bar(performed, summary) {
            self.set_focus(bar);
            self.phase = AdaptivePhase::FocusedPractice;
            let bars = self.focus_bars.clone().unwrap_or_default();
            return self.decision(
                AdaptiveAction::FocusBar,
                reduced,
                bars.clone(),
                stars,
                format!(
                    "The broader exercise is still unstable. Return to bars {} at {} BPM, with extra attention on bar {}.",
                    format_bars(&bars),
                    format_bpm(reduced),
                    bar + 1
                ),
                false,
            );
        }
        self.phase = AdaptivePhase::Recovery;
        self.decision(
            AdaptiveAction::ReduceTempo,
            reduced,
            self.original_bars.clone(),
            stars,
            format!(
                "Errors remain spread out. Reduce the full selection to {} BPM.",
                format_bpm(reduced)
            ),
            false,
        )
    }

    fn on_probe(&mut self, stars: u8, performed: &[usize], current: f64) -> AdaptiveDecision {
        if stars >= 3 && self.is_full(performed) && self.at_target(current) {
            return self.complete(
                stars,
                "Full-speed check passed at the original tempo. Adaptive learning complete.",
            );
        }
        let previous = self
            .probe_return_bpm
            .unwrap_or(self.current_bpm)
            .clamp(self.min_bpm, self.target_bpm);
        self.probe_return_bpm = None;
        self.failed_streak = 0;
        self.successful_focus_streak = 0;
        self.tempo_increases_since_probe = 0;
        self.current_bpm = previous;
        self.phase = AdaptivePhase::FullExercisePractice;
        self.decision(
            AdaptiveAction::ReturnToRecoveryTempo,
            previous,
            self.original_bars.clone(),
            stars,
            format!(
                "Full-speed check did not pass. Return to {} BPM without an extra slowdown penalty.",
                format_bpm(previous)
            ),
            false,
        )
    }

    fn complete(&mut self, stars: u8, explanation: &str) -> AdaptiveDecision {
        self.phase = AdaptivePhase::Completed;
        self.current_bpm = self.target_bpm;
        self.focus_bars = None;
        self.decision(
            AdaptiveAction::Complete,
            self.target_bpm,
            self.original_bars.clone(),
            stars,
            explanation.to_string(),
            true,
        )
    }

    fn begin_probe(&mut self, return_bpm: f64, stars: u8) -> AdaptiveDecision {
        self.probe_return_bpm = Some(return_bpm);
        self.tempo_increases_since_probe = 0;
        self.phase = AdaptivePhase::TempoProbe;
        self.decision(
            AdaptiveAction::FullSpeedProbe,
            self.target_bpm,
            self.original_bars.clone(),
            stars,
            format!(
                "{} successful tempo increases — check the full exercise at the original {} BPM.",
                self.settings.full_speed_check_interval,
                format_bpm(self.target_bpm)
            ),
            false,
        )
    }

    fn probe_due(&self, next_bpm: f64) -> bool {
        self.settings.full_speed_checks
            && self.tempo_increases_since_probe >= self.settings.full_speed_check_interval
            && next_bpm < self.target_bpm - 0.5
    }

    fn initial_recovery_bpm(&self) -> f64 {
        let target = self.target_bpm * self.settings.initial_recovery_percent / 100.0;
        round_bpm(target).clamp(self.min_bpm, self.target_bpm)
    }

    fn increase(&self, bpm: f64) -> f64 {
        round_bpm(bpm + self.settings.tempo_increase_bpm).min(self.target_bpm)
    }

    fn reduce(&self, bpm: f64) -> f64 {
        let factor = 1.0 - self.settings.secondary_reduction_percent / 100.0;
        round_bpm(bpm * factor)
            .max(self.min_bpm)
            .min(self.target_bpm)
    }

    fn at_target(&self, bpm: f64) -> bool {
        bpm >= self.target_bpm - 0.5
    }

    fn is_full(&self, bars: &[usize]) -> bool {
        bars == self.original_bars.as_slice()
    }

    fn set_focus(&mut self, problem_bar: usize) {
        let retain = self.focus_problem_bar.is_some_and(|existing| {
            self.focus_bars
                .as_ref()
                .is_some_and(|bars| bars.contains(&problem_bar) && bars.contains(&existing))
        });
        if retain {
            return;
        }
        self.focus_problem_bar = Some(problem_bar);
        self.focus_bars = Some(self.compute_focus(problem_bar));
    }

    fn compute_focus(&self, problem_bar: usize) -> Vec<usize> {
        let neighbours = self.settings.neighbour_bars;
        let low = problem_bar.saturating_sub(neighbours);
        let high = (problem_bar + neighbours).min(self.total_bars.saturating_sub(1));
        let mut bars = self
            .original_bars
            .iter()
            .copied()
            .filter(|bar| *bar >= low && *bar <= high)
            .collect::<Vec<_>>();
        if bars.is_empty() {
            bars.push(problem_bar.min(self.total_bars.saturating_sub(1)));
        }
        bars
    }

    /// Identify a primary problem bar from per-note judgements.
    ///
    /// A bar qualifies when it accounts for at least half of the unsuccessful
    /// events, either by raw count or by expected-note-normalised share. The
    /// normalised share prevents a dense bar from winning solely because it
    /// contains more notes. The current focus is retained while it still
    /// qualifies so the controller does not switch bars unnecessarily.
    fn detect_problem_bar(&self, performed: &[usize], summary: &SessionSummary) -> Option<usize> {
        if summary.results.is_empty() || performed.is_empty() {
            return None;
        }
        let performed_set = performed.iter().copied().collect::<HashSet<_>>();
        let mut errors = vec![0usize; self.total_bars];
        let mut attributed = 0usize;
        let mut unattributed = 0usize;
        for result in &summary.results {
            if !is_unsuccessful(result.judgement) {
                continue;
            }
            match result.expected_beat {
                Some(beat) => {
                    let bar = (beat / self.bar_beats).floor() as usize;
                    if bar < self.total_bars && performed_set.contains(&bar) {
                        errors[bar] += 1;
                        attributed += 1;
                    } else {
                        unattributed += 1;
                    }
                }
                None => unattributed += 1,
            }
        }
        let total_errors = attributed + unattributed;
        if total_errors == 0 {
            return None;
        }

        let total_weighted: f64 = performed
            .iter()
            .map(|bar| {
                errors.get(*bar).copied().unwrap_or(0) as f64
                    / self.expected_per_bar.get(*bar).copied().unwrap_or(0).max(1) as f64
            })
            .sum();
        if total_weighted <= 0.0 {
            return None;
        }

        let mut candidates = performed
            .iter()
            .copied()
            .filter(|bar| {
                let count = errors.get(*bar).copied().unwrap_or(0);
                if count == 0 {
                    return false;
                }
                let expected = self.expected_per_bar.get(*bar).copied().unwrap_or(0).max(1);
                let weighted = count as f64 / expected as f64;
                count * 2 >= total_errors || weighted * 2.0 >= total_weighted
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return None;
        }

        if let Some(existing) = self.focus_problem_bar
            && candidates.contains(&existing)
        {
            return Some(existing);
        }

        candidates.sort_by(|left, right| {
            let rate = |bar: &usize| {
                errors.get(*bar).copied().unwrap_or(0) as f64
                    / self.expected_per_bar.get(*bar).copied().unwrap_or(0).max(1) as f64
            };
            rate(right)
                .total_cmp(&rate(left))
                .then_with(|| {
                    errors
                        .get(*right)
                        .copied()
                        .unwrap_or(0)
                        .cmp(&errors.get(*left).copied().unwrap_or(0))
                })
                .then_with(|| left.cmp(right))
        });
        candidates.first().copied()
    }

    #[allow(clippy::too_many_arguments)]
    fn decision(
        &self,
        action: AdaptiveAction,
        bpm: f64,
        bars: Vec<usize>,
        stars: u8,
        explanation: String,
        completed: bool,
    ) -> AdaptiveDecision {
        AdaptiveDecision {
            phase: self.phase,
            action,
            bpm,
            bars,
            target_bpm: self.target_bpm,
            stars,
            explanation,
            completed,
        }
    }
}

fn is_unsuccessful(judgement: Judgement) -> bool {
    matches!(
        judgement,
        Judgement::Missed
            | Judgement::Extra
            | Judgement::WrongInstrument
            | Judgement::Early
            | Judgement::Late
    )
}

fn normalize_bars(bars: &[usize]) -> Vec<usize> {
    let mut bars = bars.to_vec();
    bars.sort_unstable();
    bars.dedup();
    bars
}

fn round_bpm(value: f64) -> f64 {
    (value * 10.0).round() / 10.0
}

fn format_bpm(value: f64) -> String {
    if (value - value.round()).abs() < 0.05 {
        format!("{:.0}", value.round())
    } else {
        format!("{value:.1}")
    }
}

fn format_percent(value: f64) -> String {
    if (value - value.round()).abs() < 0.05 {
        format!("{:.0}", value.round())
    } else {
        format!("{value:.1}")
    }
}

fn format_bars(bars: &[usize]) -> String {
    match bars {
        [] => "no bars".to_string(),
        [only] => format!("bar {}", only + 1),
        _ => format!(
            "bars {}",
            bars.iter()
                .map(|bar| (bar + 1).to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HitResult;

    fn summary(score: f64, expected: usize, missed: usize) -> SessionSummary {
        SessionSummary {
            score: Some(score),
            matched: expected.saturating_sub(missed),
            expected,
            missed,
            extras: 0,
            wrong_instrument: 0,
            mean_abs_error_ms: None,
            signed_mean_error_ms: None,
            timing_consistency_ms: None,
            scoring_version: "v4".to_string(),
            results: Vec::new(),
        }
    }

    fn result(bar: usize, judgement: Judgement, bar_beats: f64) -> HitResult {
        HitResult {
            expected_id: Some(format!("bar-{bar}")),
            expected_beat: Some(bar as f64 * bar_beats + 0.5),
            expected_timestamp_ms: None,
            observed_ms: None,
            actual_timestamp_ms: None,
            error_ms: None,
            window_ms: 100.0,
            instrument: crate::Instrument::Snare,
            midi_note: None,
            judgement,
            excellent: false,
            note_score: 0.0,
            missed: matches!(judgement, Judgement::Missed),
        }
    }

    fn attempt(
        score: f64,
        expected: usize,
        missed: usize,
        bars: &[usize],
        bpm: f64,
    ) -> AdaptiveAttempt {
        AdaptiveAttempt {
            summary: summary(score, expected, missed),
            performed_bars: bars.to_vec(),
            current_bpm: bpm,
        }
    }

    fn make_controller() -> AdaptiveController {
        AdaptiveController::new(
            AdaptiveSettings::default(),
            120.0,
            vec![0, 1, 2, 3],
            4,
            vec![4, 4, 4, 4],
            4.0,
        )
    }

    #[test]
    fn initial_recovery_examples_match_specification() {
        for (target, expected) in [(160.0, 80.0), (120.0, 60.0), (100.0, 50.0), (80.0, 50.0)] {
            let mut controller = AdaptiveController::new(
                AdaptiveSettings::default(),
                target,
                vec![0],
                1,
                vec![4],
                4.0,
            );
            controller.start();
            let decision = controller.observe(&attempt(40.0, 4, 4, &[0], target));
            assert_eq!(decision.bpm, expected, "target {target}");
            assert_eq!(decision.action, AdaptiveAction::ReduceTempo);
        }
    }

    #[test]
    fn original_tempo_below_minimum_is_respected() {
        let mut controller = AdaptiveController::new(
            AdaptiveSettings {
                min_bpm: 100.0,
                ..AdaptiveSettings::default()
            },
            60.0,
            vec![0],
            1,
            vec![4],
            4.0,
        );
        controller.start();
        let decision = controller.observe(&attempt(40.0, 4, 4, &[0], 60.0));
        assert_eq!(decision.bpm, 60.0);
    }

    #[test]
    fn three_stars_at_target_completes() {
        let mut controller = make_controller();
        controller.start();
        let decision = controller.observe(&attempt(100.0, 16, 0, &[0, 1, 2, 3], 120.0));
        assert!(decision.completed);
        assert_eq!(decision.phase, AdaptivePhase::Completed);
        assert_eq!(decision.action, AdaptiveAction::Complete);
    }

    #[test]
    fn two_stars_repeats_at_current_tempo() {
        let mut controller = make_controller();
        controller.start();
        let decision = controller.observe(&attempt(80.0, 16, 1, &[0, 1, 2, 3], 120.0));
        assert_eq!(decision.action, AdaptiveAction::Repeat);
        assert_eq!(decision.bpm, 120.0);
        assert_eq!(decision.phase, AdaptivePhase::InitialAssessment);
    }

    #[test]
    fn zero_or_one_star_enters_recovery() {
        for score in [0.0, 40.0, 55.0] {
            let mut controller = make_controller();
            controller.start();
            let decision = controller.observe(&attempt(score, 16, 8, &[0, 1, 2, 3], 120.0));
            assert_eq!(decision.phase, AdaptivePhase::Recovery);
            assert_eq!(decision.bpm, 60.0);
        }
    }

    #[test]
    fn failed_attempt_threshold_delays_secondary_reduction() {
        let mut controller = make_controller();
        controller.start();
        controller.observe(&attempt(40.0, 16, 8, &[0, 1, 2, 3], 120.0));
        // First failure after recovery repeats at the same tempo.
        let decision = controller.observe(&attempt(40.0, 16, 8, &[0, 1, 2, 3], 60.0));
        assert_eq!(decision.action, AdaptiveAction::Repeat);
        assert_eq!(decision.bpm, 60.0);
        // Second failure reduces by 10%.
        let decision = controller.observe(&attempt(40.0, 16, 8, &[0, 1, 2, 3], 60.0));
        assert_eq!(decision.action, AdaptiveAction::ReduceTempo);
        assert_eq!(decision.bpm, 54.0);
    }

    #[test]
    fn tempo_increases_by_setting_and_caps_at_target() {
        let mut controller = make_controller();
        controller.start();
        controller.observe(&attempt(40.0, 16, 8, &[0, 1, 2, 3], 120.0));
        let decision = controller.observe(&attempt(100.0, 16, 0, &[0, 1, 2, 3], 60.0));
        assert_eq!(decision.bpm, 65.0);
        assert_eq!(decision.action, AdaptiveAction::IncreaseTempo);

        // Force a high tempo near the target.
        let mut controller = make_controller();
        controller.start();
        controller.observe(&attempt(40.0, 16, 8, &[0, 1, 2, 3], 120.0));
        let decision = controller.observe(&attempt(100.0, 16, 0, &[0, 1, 2, 3], 118.0));
        assert_eq!(decision.bpm, 120.0);
    }

    #[test]
    fn never_reduces_below_minimum() {
        let mut controller = AdaptiveController::new(
            AdaptiveSettings {
                min_bpm: 55.0,
                ..AdaptiveSettings::default()
            },
            120.0,
            vec![0],
            1,
            vec![4],
            4.0,
        );
        controller.start();
        controller.observe(&attempt(40.0, 4, 4, &[0], 120.0));
        let mut bpm = 60.0;
        for _ in 0..10 {
            let decision = controller.observe(&attempt(40.0, 4, 4, &[0], bpm));
            bpm = decision.bpm;
            assert!(bpm >= 55.0);
        }
    }

    #[test]
    fn problem_bar_is_identified_and_neighbours_are_included() {
        let mut controller = make_controller();
        controller.start();
        let mut summary = summary(40.0, 16, 8);
        for bar in 0..4 {
            let count = if bar == 2 { 8 } else { 0 };
            for _ in 0..count {
                summary.results.push(result(bar, Judgement::Missed, 4.0));
            }
        }
        let decision = controller.observe(&AdaptiveAttempt {
            summary,
            performed_bars: vec![0, 1, 2, 3],
            current_bpm: 120.0,
        });
        assert_eq!(decision.phase, AdaptivePhase::FocusedPractice);
        assert_eq!(decision.action, AdaptiveAction::FocusBar);
        assert_eq!(decision.bars, vec![1, 2, 3]);
    }

    #[test]
    fn focused_selection_respects_user_subset_and_boundaries() {
        let mut controller = AdaptiveController::new(
            AdaptiveSettings::default(),
            120.0,
            vec![0, 2, 3],
            5,
            vec![4, 4, 4, 4, 4],
            4.0,
        );
        controller.start();
        let mut summary = summary(40.0, 12, 6);
        for _ in 0..6 {
            summary.results.push(result(2, Judgement::Missed, 4.0));
        }
        let decision = controller.observe(&AdaptiveAttempt {
            summary,
            performed_bars: vec![0, 2, 3],
            current_bpm: 120.0,
        });
        // Bar 2's neighbours are 1..3, intersected with the user's selection.
        assert_eq!(decision.bars, vec![2, 3]);
    }

    #[test]
    fn distributed_errors_keep_broader_selection() {
        let mut controller = make_controller();
        controller.start();
        let mut summary = summary(40.0, 16, 8);
        for bar in 0..4 {
            for _ in 0..2 {
                summary.results.push(result(bar, Judgement::Missed, 4.0));
            }
        }
        let decision = controller.observe(&AdaptiveAttempt {
            summary,
            performed_bars: vec![0, 1, 2, 3],
            current_bpm: 120.0,
        });
        assert_eq!(decision.phase, AdaptivePhase::Recovery);
        assert_eq!(decision.bars, vec![0, 1, 2, 3]);
    }

    #[test]
    fn dense_bar_does_not_win_solely_on_note_count() {
        let mut controller = AdaptiveController::new(
            AdaptiveSettings::default(),
            120.0,
            vec![0, 1, 2, 3],
            4,
            vec![20, 4, 4, 4],
            4.0,
        );
        controller.start();
        let mut summary = summary(40.0, 32, 12);
        // Dense bar 0: 6 errors out of 20 expected (30%).
        for _ in 0..6 {
            summary.results.push(result(0, Judgement::Missed, 4.0));
        }
        // Sparse bar 2: 4 errors out of 4 expected (100%).
        for _ in 0..4 {
            summary.results.push(result(2, Judgement::Missed, 4.0));
        }
        let decision = controller.observe(&AdaptiveAttempt {
            summary,
            performed_bars: vec![0, 1, 2, 3],
            current_bpm: 120.0,
        });
        assert_eq!(decision.phase, AdaptivePhase::FocusedPractice);
        assert_eq!(decision.bars, vec![1, 2, 3]);
    }

    #[test]
    fn two_focused_successes_restore_full_selection() {
        let mut controller = make_controller();
        controller.start();
        let mut summary = summary(40.0, 16, 8);
        for _ in 0..8 {
            summary.results.push(result(2, Judgement::Missed, 4.0));
        }
        let decision = controller.observe(&AdaptiveAttempt {
            summary,
            performed_bars: vec![0, 1, 2, 3],
            current_bpm: 120.0,
        });
        assert_eq!(decision.phase, AdaptivePhase::FocusedPractice);
        let focus = decision.bars.clone();

        let first = controller.observe(&attempt(100.0, 12, 0, &focus, decision.bpm));
        assert_eq!(first.phase, AdaptivePhase::FocusedPractice);
        let second = controller.observe(&attempt(100.0, 12, 0, &focus, first.bpm));
        assert_eq!(second.phase, AdaptivePhase::FullExercisePractice);
        assert_eq!(second.action, AdaptiveAction::ReturnToFullExercise);
        assert_eq!(second.bars, vec![0, 1, 2, 3]);
    }

    #[test]
    fn full_speed_probe_after_configured_increases_and_returns_to_previous_tempo() {
        let mut controller = AdaptiveController::new(
            AdaptiveSettings {
                full_speed_check_interval: 1,
                ..AdaptiveSettings::default()
            },
            120.0,
            vec![0],
            1,
            vec![4],
            4.0,
        );
        controller.start();
        controller.observe(&attempt(40.0, 4, 4, &[0], 120.0));
        let probe = controller.observe(&attempt(100.0, 4, 0, &[0], 60.0));
        assert_eq!(probe.phase, AdaptivePhase::TempoProbe);
        assert_eq!(probe.action, AdaptiveAction::FullSpeedProbe);
        assert_eq!(probe.bpm, 120.0);

        let returned = controller.observe(&attempt(80.0, 4, 1, &[0], 120.0));
        assert_eq!(returned.phase, AdaptivePhase::FullExercisePractice);
        assert_eq!(returned.action, AdaptiveAction::ReturnToRecoveryTempo);
        assert_eq!(returned.bpm, 65.0);
    }

    #[test]
    fn completion_requires_full_selection_at_target() {
        let mut controller = make_controller();
        controller.start();
        // A focused passage at target is not completion.
        let mut summary = summary(40.0, 16, 8);
        for _ in 0..8 {
            summary.results.push(result(2, Judgement::Missed, 4.0));
        }
        let decision = controller.observe(&AdaptiveAttempt {
            summary,
            performed_bars: vec![0, 1, 2, 3],
            current_bpm: 120.0,
        });
        let focused = controller.observe(&attempt(100.0, 12, 0, &decision.bars, decision.bpm));
        assert!(!focused.completed);
    }

    #[test]
    fn settings_normalize_to_documented_ranges() {
        let normalized = AdaptiveSettings {
            initial_recovery_percent: 10.0,
            min_bpm: 500.0,
            tempo_increase_bpm: 50.0,
            secondary_reduction_percent: 90.0,
            failed_attempts_before_slow: 0,
            successful_repetitions_required: 9,
            neighbour_bars: 8,
            full_speed_checks: false,
            full_speed_check_interval: 99,
        }
        .normalized();
        assert_eq!(normalized.initial_recovery_percent, 30.0);
        assert_eq!(normalized.min_bpm, 100.0);
        assert_eq!(normalized.tempo_increase_bpm, 20.0);
        assert_eq!(normalized.secondary_reduction_percent, 30.0);
        assert_eq!(normalized.failed_attempts_before_slow, 1);
        assert_eq!(normalized.successful_repetitions_required, 5);
        assert_eq!(normalized.neighbour_bars, 2);
        assert_eq!(normalized.full_speed_check_interval, 10);
    }

    #[test]
    fn reset_returns_to_initial_assessment_on_next_start() {
        let mut controller = make_controller();
        controller.start();
        controller.observe(&attempt(40.0, 16, 8, &[0, 1, 2, 3], 120.0));
        assert_eq!(controller.phase(), AdaptivePhase::Recovery);
        controller.reset();
        assert_eq!(controller.phase(), AdaptivePhase::Inactive);
        let decision = controller.start();
        assert_eq!(decision.phase, AdaptivePhase::InitialAssessment);
        assert_eq!(decision.bpm, 120.0);
        assert_eq!(decision.bars, vec![0, 1, 2, 3]);
    }
}
