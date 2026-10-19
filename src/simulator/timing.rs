//! Lap, sector and personal-best timing — port of ksengine
//! `engine/physics/phys_LapTimer.*` and `engine/physics/LapSectorTimer.h`
//! (GPL-3.0). Original: <https://github.com/SimonTek27/ksengine>
//!
//! [`LapTimer`] mirrors the event-driven `phys_LapTimer`: the caller reports
//! completed sectors and laps and the timer keeps the running lap, the best lap
//! and the history. [`LapSectorTimer`] is the spline-based timer that derives the
//! sector boundaries and the start/finish crossing from the normalized spline
//! position, so it works straight from a physics loop.
//!
//! > The C++ `phys_LapTimer` stores two `std::function` callbacks
//! > (`onSectorCompleted`, `onLapCompleted`). Rust ports expose the same data
//! > through the getters instead of a callback pair, which keeps the timer
//! > `Default` + testable; callers poll [`LapTimer::lap_count`] and
//! > [`LapTimer::lap_history`] to react.

/// Estimated lap time and how much to trust it (port of `LapTimerEstimate`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LapTimerEstimate {
    /// Mean of the sampled lap times, in seconds (`0` when there is no sample).
    pub estimated_lap: f64,
    /// `1 / (1 + sqrt(variance))` — `1` for a perfectly consistent sample.
    pub confidence: f64,
}

/// Event-driven lap timer (port of `phys_LapTimer`).
///
/// # Example
///
/// ```
/// use racingedge::simulator::timing::LapTimer;
/// let mut timer = LapTimer::new();
/// timer.complete_lap(83.5);
/// timer.complete_lap(82.1);
/// assert_eq!(timer.lap_count(), 2);
/// assert_eq!(timer.best_lap_time(), 82.1);
/// ```
#[derive(Clone, Debug, Default)]
pub struct LapTimer {
    current_lap_time: f64,
    best_lap: f64,
    lap_count: i32,
    sectors: Vec<f64>,
    history: Vec<f64>,
    sector1_time: f64,
    sector2_time: f64,
}

impl LapTimer {
    /// A fresh timer with no laps.
    pub fn new() -> Self {
        Self::default()
    }

    /// Clears every lap, sector and history record.
    pub fn reset(&mut self) {
        self.current_lap_time = 0.0;
        self.best_lap = 0.0;
        self.lap_count = 0;
        self.sectors.clear();
        self.history.clear();
        self.sector1_time = 0.0;
        self.sector2_time = 0.0;
    }

    /// Starts a new lap: resets the running time and the sector splits, but
    /// keeps the best lap and the history.
    pub fn start_lap(&mut self) {
        self.current_lap_time = 0.0;
        self.sectors.clear();
        self.sector1_time = 0.0;
        self.sector2_time = 0.0;
    }

    /// Records the cumulative time at which `sector_index` (1-based, as in the
    /// reference) was completed.
    pub fn complete_sector(&mut self, sector_index: usize, time_sec: f64) {
        if sector_index == 1 {
            self.sector1_time = time_sec;
        }
        if sector_index == 2 {
            self.sector2_time = time_sec;
        }
        if self.sectors.len() < sector_index {
            self.sectors.resize(sector_index, 0.0);
        }
        if sector_index > 0 {
            self.sectors[sector_index - 1] = time_sec;
        }
    }

    /// Records a completed lap and folds it into the best lap and history.
    pub fn complete_lap(&mut self, lap_time_sec: f64) {
        self.current_lap_time = lap_time_sec;
        self.history.push(lap_time_sec);
        self.lap_count += 1;
        if self.best_lap <= 0.0 || lap_time_sec < self.best_lap {
            self.best_lap = lap_time_sec;
        }
    }

    /// Running lap time of the lap in progress, in seconds.
    pub fn current_lap_time(&self) -> f64 {
        self.current_lap_time
    }

    /// Fastest completed lap, in seconds (`0` until a lap is completed).
    pub fn best_lap_time(&self) -> f64 {
        self.best_lap
    }

    /// Number of completed laps.
    pub fn lap_count(&self) -> i32 {
        self.lap_count
    }

    /// Cumulative sector splits of the current lap (sector `i` at index `i`).
    pub fn sector_times(&self) -> &[f64] {
        &self.sectors
    }

    /// Every completed lap time, in completion order.
    pub fn lap_history(&self) -> &[f64] {
        &self.history
    }

    /// Cumulative time at which sector 1 was completed.
    pub fn sector1_time(&self) -> f64 {
        self.sector1_time
    }

    /// Cumulative time at which sector 2 was completed.
    pub fn sector2_time(&self) -> f64 {
        self.sector2_time
    }

    /// Estimates the next lap time from `historical_lap_times`, or from the
    /// timer's own history when the slice is empty.
    pub fn estimate_lap_time(&self, historical_lap_times: &[f64]) -> LapTimerEstimate {
        let src = if historical_lap_times.is_empty() {
            self.history.as_slice()
        } else {
            historical_lap_times
        };
        if src.is_empty() {
            return LapTimerEstimate::default();
        }

        let estimated_lap = src.iter().sum::<f64>() / src.len() as f64;
        let variance = src
            .iter()
            .map(|t| {
                let d = t - estimated_lap;
                d * d
            })
            .sum::<f64>()
            / src.len() as f64;

        LapTimerEstimate {
            estimated_lap,
            confidence: 1.0 / (1.0 + variance.sqrt()),
        }
    }
}

/// Spline-based lap/sector timer (port of `LapSectorTimer`).
///
/// Sector splits are normalized spline positions in `[0, 1)`. A lap is counted
/// when the spline wraps from `> 0.85` to `< 0.15`.
#[derive(Clone, Debug)]
pub struct LapSectorTimer {
    sector_count: i32,
    sector_splits: Vec<f32>,
    sector_times: Vec<i32>,
    best_sectors: Vec<i32>,
    lap_ms: i32,
    last_lap_ms: i32,
    best_lap_ms: i32,
    sector_index: i32,
    completed_laps: i32,
    prev_spline: f32,
    running: bool,
}

impl Default for LapSectorTimer {
    fn default() -> Self {
        let mut timer = Self {
            sector_count: 0,
            sector_splits: Vec::new(),
            sector_times: Vec::new(),
            best_sectors: Vec::new(),
            lap_ms: 0,
            last_lap_ms: 0,
            best_lap_ms: 0,
            sector_index: 0,
            completed_laps: 0,
            prev_spline: 0.0,
            running: false,
        };
        timer.configure(3);
        timer
    }
}

impl LapSectorTimer {
    /// A timer with `sector_count` equal split sectors (default 3).
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets `sector_count` equal split sectors and resets all timing.
    pub fn configure(&mut self, sector_count: i32) {
        self.sector_count = sector_count.max(1);
        self.sector_splits = (0..self.sector_count)
            .map(|i| i as f32 / self.sector_count as f32)
            .collect();
        self.reset();
    }

    /// Sets custom splits (ascending in `[0, 1)`) and resets all timing.
    /// An empty vector is ignored.
    pub fn set_splits(&mut self, mut splits: Vec<f32>) {
        if splits.is_empty() {
            return;
        }
        splits.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        self.sector_count = splits.len() as i32;
        self.sector_splits = splits;
        self.reset();
    }

    /// Clears every lap and sector record. The splits are kept.
    pub fn reset(&mut self) {
        self.lap_ms = 0;
        self.last_lap_ms = 0;
        self.best_lap_ms = 0;
        self.sector_index = 0;
        self.completed_laps = 0;
        self.prev_spline = 0.0;
        self.running = false;
        self.sector_times = vec![0; self.sector_count as usize];
        self.best_sectors = vec![0; self.sector_count as usize];
    }

    /// Starts (or restarts) the current lap.
    pub fn start(&mut self) {
        self.running = true;
        self.lap_ms = 0;
        self.sector_index = 0;
    }

    /// Stops timing. The accumulated values are kept.
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// Advances the timer by one physics step.
    ///
    /// `normalized_spline` is the car's position along the track centerline in
    /// `[0, 1]`; a wrap from `> 0.85` to `< 0.15` completes the lap.
    pub fn update(&mut self, dt_sec: f64, normalized_spline: f32) {
        if !self.running {
            return;
        }
        self.lap_ms += (dt_sec * 1000.0) as i32;

        if self.prev_spline > 0.85 && normalized_spline < 0.15 {
            self.on_lap_complete();
        } else {
            let next = (self.sector_index + 1) % self.sector_count;
            let split = self.sector_splits[next as usize];
            if next != 0 && self.prev_spline < split && normalized_spline >= split {
                self.sector_times[self.sector_index as usize] = self.lap_ms;
                let best = self.best_sectors[self.sector_index as usize];
                if best == 0 || self.lap_ms < best {
                    self.best_sectors[self.sector_index as usize] = self.lap_ms;
                }
                self.sector_index = next;
            }
        }

        self.prev_spline = normalized_spline;
    }

    /// Running lap time in milliseconds.
    pub fn current_time_ms(&self) -> i32 {
        self.lap_ms
    }

    /// Last completed lap time in milliseconds.
    pub fn last_time_ms(&self) -> i32 {
        self.last_lap_ms
    }

    /// Best completed lap time in milliseconds (`0` until a lap is completed).
    pub fn best_time_ms(&self) -> i32 {
        self.best_lap_ms
    }

    /// Index of the sector currently in progress.
    pub fn sector_index(&self) -> i32 {
        self.sector_index
    }

    /// Number of completed laps.
    pub fn completed_laps(&self) -> i32 {
        self.completed_laps
    }

    /// Number of sectors the lap is split into.
    pub fn sector_count(&self) -> i32 {
        self.sector_count
    }

    /// Whether the timer is running.
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Cumulative sector splits of the last lap, in milliseconds.
    pub fn sector_times(&self) -> &[i32] {
        &self.sector_times
    }

    /// Best cumulative sector splits, in milliseconds.
    pub fn best_sector_times(&self) -> &[i32] {
        &self.best_sectors
    }

    /// Normalized spline positions of the sector boundaries.
    pub fn sector_splits(&self) -> &[f32] {
        &self.sector_splits
    }

    fn on_lap_complete(&mut self) {
        self.last_lap_ms = self.lap_ms;
        if self.best_lap_ms == 0 || self.lap_ms < self.best_lap_ms {
            self.best_lap_ms = self.lap_ms;
        }
        self.sector_times[self.sector_index as usize] = self.lap_ms;
        self.completed_laps += 1;
        self.lap_ms = 0;
        self.sector_index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lapper_default_is_empty() {
        let timer = LapTimer::new();
        assert_eq!(timer.lap_count(), 0);
        assert_eq!(timer.best_lap_time(), 0.0);
        assert_eq!(timer.current_lap_time(), 0.0);
        assert!(timer.lap_history().is_empty());
        assert!(timer.sector_times().is_empty());
    }

    #[test]
    fn lapper_tracks_best_and_history() {
        let mut timer = LapTimer::new();
        timer.complete_lap(83.5);
        timer.complete_lap(82.1);
        timer.complete_lap(84.0);
        assert_eq!(timer.lap_count(), 3);
        assert_eq!(timer.best_lap_time(), 82.1);
        assert_eq!(timer.current_lap_time(), 84.0);
        assert_eq!(timer.lap_history(), &[83.5, 82.1, 84.0]);
    }

    #[test]
    fn lapper_complete_sector_uses_one_based_index() {
        let mut timer = LapTimer::new();
        timer.complete_sector(1, 28.0);
        timer.complete_sector(2, 55.0);
        timer.complete_sector(3, 82.0);
        assert_eq!(timer.sector1_time(), 28.0);
        assert_eq!(timer.sector2_time(), 55.0);
        assert_eq!(timer.sector_times(), &[28.0, 55.0, 82.0]);
    }

    #[test]
    fn lapper_start_lap_clears_sectors_but_keeps_best() {
        let mut timer = LapTimer::new();
        timer.complete_lap(80.0);
        timer.complete_sector(1, 25.0);
        timer.start_lap();
        assert_eq!(timer.best_lap_time(), 80.0);
        assert!(timer.sector_times().is_empty());
        assert_eq!(timer.current_lap_time(), 0.0);
    }

    #[test]
    fn lapper_reset_clears_everything() {
        let mut timer = LapTimer::new();
        timer.complete_lap(80.0);
        timer.reset();
        assert_eq!(timer.lap_count(), 0);
        assert_eq!(timer.best_lap_time(), 0.0);
        assert!(timer.lap_history().is_empty());
    }

    #[test]
    fn lapper_estimate_uses_own_history_when_empty() {
        let mut timer = LapTimer::new();
        timer.complete_lap(80.0);
        timer.complete_lap(82.0);
        let estimate = timer.estimate_lap_time(&[]);
        assert_eq!(estimate.estimated_lap, 81.0);
        assert!(estimate.confidence > 0.0 && estimate.confidence < 1.0);
    }

    #[test]
    fn lapper_estimate_prefers_explicit_sample() {
        let mut timer = LapTimer::new();
        timer.complete_lap(80.0);
        let estimate = timer.estimate_lap_time(&[100.0, 100.0]);
        assert_eq!(estimate.estimated_lap, 100.0);
        assert_eq!(estimate.confidence, 1.0);
    }

    #[test]
    fn lapper_estimate_empty_is_zero() {
        let timer = LapTimer::new();
        assert_eq!(timer.estimate_lap_time(&[]), LapTimerEstimate::default());
    }

    #[test]
    fn sector_timer_default_has_three_equal_sectors() {
        let timer = LapSectorTimer::new();
        assert_eq!(timer.sector_count(), 3);
        assert_eq!(timer.sector_splits(), &[0.0, 1.0 / 3.0, 2.0 / 3.0]);
    }

    #[test]
    fn sector_timer_set_splits_sorts_and_resets() {
        let mut timer = LapSectorTimer::new();
        timer.start();
        timer.set_splits(vec![0.6, 0.2]);
        assert_eq!(timer.sector_count(), 2);
        assert_eq!(timer.sector_splits(), &[0.2, 0.6]);
        assert!(!timer.is_running());
        assert_eq!(timer.sector_times().len(), 2);
    }

    #[test]
    fn sector_timer_ignores_empty_splits() {
        let mut timer = LapSectorTimer::new();
        timer.set_splits(Vec::new());
        assert_eq!(timer.sector_count(), 3);
    }

    #[test]
    fn sector_timer_accumulates_lap_time() {
        let mut timer = LapSectorTimer::new();
        timer.start();
        timer.update(0.5, 0.01);
        timer.update(0.5, 0.02);
        assert_eq!(timer.current_time_ms(), 1000);
    }

    #[test]
    fn sector_timer_detects_sector_boundary() {
        let mut timer = LapSectorTimer::new();
        timer.start();
        // Cross the first split (1/3) within a single lap.
        timer.update(1.0, 0.20);
        timer.update(1.0, 0.40);
        assert_eq!(timer.sector_index(), 1);
        assert_eq!(timer.sector_times()[0], 2000);
    }

    #[test]
    fn sector_timer_counts_lap_on_spline_wrap() {
        let mut timer = LapSectorTimer::new();
        timer.start();
        timer.update(1.0, 0.90);
        timer.update(1.0, 0.05);
        assert_eq!(timer.completed_laps(), 1);
        assert_eq!(timer.last_time_ms(), 2000);
        assert_eq!(timer.best_time_ms(), 2000);
        assert_eq!(timer.current_time_ms(), 0);
        assert_eq!(timer.sector_index(), 0);
    }

    #[test]
    fn sector_timer_keeps_best_lap() {
        let mut timer = LapSectorTimer::new();
        timer.start();
        timer.update(10.0, 0.9);
        timer.update(1.0, 0.05);
        timer.update(10.0, 0.9);
        timer.update(1.0, 0.05);
        assert_eq!(timer.completed_laps(), 2);
        assert_eq!(timer.best_time_ms(), 11000);
        assert_eq!(timer.last_time_ms(), 11000);
    }

    #[test]
    fn sector_timer_ignores_updates_when_stopped() {
        let mut timer = LapSectorTimer::new();
        timer.update(1.0, 0.5);
        assert_eq!(timer.current_time_ms(), 0);
    }
}
