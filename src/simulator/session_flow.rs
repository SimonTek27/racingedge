//! Session flow state machine — port of ksengine
//! `src/simulator/SessionFlow.h` (GPL-3.0).
//! Original: <https://github.com/SimonTek27/ksengine>
//!
//! Drives Qualifying → GridPublished → Countdown → Race → Finished, with the
//! qualifying times used to publish a grid and the final standings sorted at
//! the finish. The reference writes to `stderr` for tracing; the port drops
//! those prints and exposes the same state through the getters.

/// Current phase of the session flow (port of `SessionFlowPhase`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SessionFlowPhase {
    /// Not started.
    #[default]
    Idle = 0,
    /// Qualifying is running.
    Qualifying = 1,
    /// Grid has just been published.
    GridPublished = 2,
    /// Race countdown.
    Countdown = 3,
    /// Green flag.
    Race = 4,
    /// Results available.
    Finished = 5,
}

impl SessionFlowPhase {
    /// Short name (port of `sessionFlowPhaseName`).
    pub fn name(self) -> &'static str {
        match self {
            SessionFlowPhase::Idle => "Idle",
            SessionFlowPhase::Qualifying => "Qualifying",
            SessionFlowPhase::GridPublished => "Grid",
            SessionFlowPhase::Countdown => "Countdown",
            SessionFlowPhase::Race => "Race",
            SessionFlowPhase::Finished => "Finished",
        }
    }
}

/// A qualifying entry (port of `QualyEntry`).
#[derive(Clone, Debug, PartialEq)]
pub struct QualyEntry {
    /// Car index.
    pub car_id: i32,
    /// Driver name.
    pub name: String,
    /// Best qualifying lap, in seconds (`1e9` when unset).
    pub best_lap: f32,
    /// Grid position once published (`0` before that).
    pub grid_pos: i32,
}

impl QualyEntry {
    /// A qualifying entry for `car_id` with no time set yet.
    pub fn new(car_id: i32, name: impl Into<String>) -> Self {
        Self {
            car_id,
            name: name.into(),
            best_lap: 1e9,
            grid_pos: 0,
        }
    }
}

/// A final classification entry (port of `RaceResultEntry`).
#[derive(Clone, Debug, PartialEq)]
pub struct RaceResultEntry {
    /// Car index.
    pub car_id: i32,
    /// Driver name.
    pub name: String,
    /// Final position (`0` before the standings are sorted).
    pub position: i32,
    /// Laps completed.
    pub laps: i32,
    /// Total elapsed time, in seconds.
    pub total_time: f32,
    /// Best lap, in seconds (`1e9` when unset).
    pub best_lap: f32,
    /// Whether the car took the checkered flag.
    pub finished: bool,
    /// Did not finish.
    pub dnf: bool,
}

impl RaceResultEntry {
    /// A result entry for `car_id` with no time set yet.
    pub fn new(car_id: i32, name: impl Into<String>) -> Self {
        Self {
            car_id,
            name: name.into(),
            position: 0,
            laps: 0,
            total_time: 0.0,
            best_lap: 1e9,
            finished: false,
            dnf: false,
        }
    }
}

/// Qualifying → race flow (port of `SessionFlow`).
///
/// # Example
///
/// ```
/// use racingedge::simulator::session_flow::{QualyEntry, SessionFlow, SessionFlowPhase};
/// let mut flow = SessionFlow::new();
/// flow.start_qualifying(120.0, vec![QualyEntry::new(1, "a"), QualyEntry::new(2, "b")]);
/// flow.report_qualy_lap(2, 80.0);
/// flow.report_qualy_lap(1, 79.0);
/// flow.tick(120.0); // qualifying time is up → grid published
/// assert_eq!(flow.phase(), SessionFlowPhase::GridPublished);
/// assert_eq!(flow.qualy()[0].car_id, 1);
/// assert_eq!(flow.qualy()[0].grid_pos, 1);
/// ```
#[derive(Clone, Debug)]
pub struct SessionFlow {
    phase: SessionFlowPhase,
    phase_time: f32,
    qualy_duration: f32,
    countdown: f32,
    total_laps: i32,
    qualy: Vec<QualyEntry>,
    results: Vec<RaceResultEntry>,
}

impl Default for SessionFlow {
    fn default() -> Self {
        Self {
            phase: SessionFlowPhase::Idle,
            phase_time: 0.0,
            qualy_duration: 600.0,
            countdown: 0.0,
            total_laps: 5,
            qualy: Vec::new(),
            results: Vec::new(),
        }
    }
}

impl SessionFlow {
    /// A fresh flow in [`SessionFlowPhase::Idle`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns to [`SessionFlowPhase::Idle`] and clears every entry.
    pub fn reset(&mut self) {
        self.phase = SessionFlowPhase::Idle;
        self.qualy.clear();
        self.results.clear();
        self.countdown = 0.0;
        self.phase_time = 0.0;
    }

    /// Starts qualifying for at least 30 seconds with `cars` on the entry list.
    pub fn start_qualifying(&mut self, duration_sec: f32, cars: Vec<QualyEntry>) {
        self.phase = SessionFlowPhase::Qualifying;
        self.phase_time = 0.0;
        self.qualy_duration = duration_sec.max(30.0);
        self.qualy = cars;
        for entry in &mut self.qualy {
            entry.best_lap = 1e9;
            entry.grid_pos = 0;
        }
    }

    /// Skips qualifying and goes straight to a countdown of `countdown_sec`.
    pub fn start_race_direct(&mut self, total_laps: i32, countdown_sec: f32) {
        self.total_laps = total_laps.max(1);
        self.phase = SessionFlowPhase::Countdown;
        self.countdown = countdown_sec.max(1.0);
        self.phase_time = 0.0;
    }

    /// Records a qualifying lap if qualifying is running and the time improves.
    pub fn report_qualy_lap(&mut self, car_id: i32, lap_time: f32) {
        if self.phase != SessionFlowPhase::Qualifying {
            return;
        }
        if !lap_time.is_finite() || lap_time <= 1.0 {
            return;
        }
        for entry in &mut self.qualy {
            if entry.car_id == car_id && lap_time < entry.best_lap {
                entry.best_lap = lap_time;
            }
        }
    }

    /// Advances the flow by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        if matches!(
            self.phase,
            SessionFlowPhase::Idle | SessionFlowPhase::Finished
        ) {
            return;
        }
        self.phase_time += dt;
        match self.phase {
            SessionFlowPhase::Qualifying => {
                if self.phase_time >= self.qualy_duration {
                    self.publish_grid();
                }
            }
            SessionFlowPhase::GridPublished => {
                if self.phase_time >= 2.0 {
                    self.enter_countdown(5.0);
                }
            }
            SessionFlowPhase::Countdown => {
                self.countdown -= dt;
                if self.countdown <= 0.0 {
                    self.countdown = 0.0;
                    self.phase = SessionFlowPhase::Race;
                    self.phase_time = 0.0;
                }
            }
            _ => {}
        }
    }

    /// Sorts the qualifying list by best lap and assigns grid positions.
    pub fn publish_grid(&mut self) {
        self.qualy.sort_by(|a, b| {
            let a_ok = a.best_lap < 1e8;
            let b_ok = b.best_lap < 1e8;
            if a_ok != b_ok {
                return b_ok.cmp(&a_ok);
            }
            a.best_lap
                .partial_cmp(&b.best_lap)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (i, entry) in self.qualy.iter_mut().enumerate() {
            entry.grid_pos = i as i32 + 1;
        }
        self.phase = SessionFlowPhase::GridPublished;
        self.phase_time = 0.0;
    }

    /// Enters the countdown with at least a one-second duration.
    pub fn enter_countdown(&mut self, sec: f32) {
        self.phase = SessionFlowPhase::Countdown;
        self.countdown = sec.max(1.0);
        self.phase_time = 0.0;
    }

    /// Publishes the final standings and moves to [`SessionFlowPhase::Finished`].
    pub fn finish_race(&mut self, standings: Vec<RaceResultEntry>) {
        self.results = standings;
        self.results.sort_by(|a, b| {
            if a.finished != b.finished {
                return if a.finished {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            if a.laps != b.laps {
                return b.laps.cmp(&a.laps);
            }
            a.total_time
                .partial_cmp(&b.total_time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        for (i, entry) in self.results.iter_mut().enumerate() {
            entry.position = i as i32 + 1;
        }
        self.phase = SessionFlowPhase::Finished;
    }

    /// Current phase.
    pub fn phase(&self) -> SessionFlowPhase {
        self.phase
    }

    /// Remaining countdown seconds.
    pub fn countdown(&self) -> f32 {
        self.countdown
    }

    /// Seconds elapsed in the current phase.
    pub fn phase_time(&self) -> f32 {
        self.phase_time
    }

    /// Total race laps.
    pub fn total_laps(&self) -> i32 {
        self.total_laps
    }

    /// The qualifying entry list.
    pub fn qualy(&self) -> &[QualyEntry] {
        &self.qualy
    }

    /// The final classification.
    pub fn results(&self) -> &[RaceResultEntry] {
        &self.results
    }

    /// Whether the race is green.
    pub fn is_green(&self) -> bool {
        self.phase == SessionFlowPhase::Race
    }

    /// Whether the current phase blocks driving.
    pub fn blocks_drive(&self) -> bool {
        matches!(
            self.phase,
            SessionFlowPhase::Countdown
                | SessionFlowPhase::GridPublished
                | SessionFlowPhase::Finished
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phase_names_match_reference() {
        assert_eq!(SessionFlowPhase::Idle.name(), "Idle");
        assert_eq!(SessionFlowPhase::GridPublished.name(), "Grid");
        assert_eq!(SessionFlowPhase::Countdown.name(), "Countdown");
        assert_eq!(SessionFlowPhase::Finished.name(), "Finished");
    }

    #[test]
    fn qualifying_publishes_grid_by_best_lap() {
        let mut flow = SessionFlow::new();
        flow.start_qualifying(
            60.0,
            vec![
                QualyEntry::new(1, "slow"),
                QualyEntry::new(2, "fast"),
                QualyEntry::new(3, "no-time"),
            ],
        );
        flow.report_qualy_lap(1, 82.0);
        flow.report_qualy_lap(2, 80.5);
        flow.tick(60.0);

        assert_eq!(flow.phase(), SessionFlowPhase::GridPublished);
        assert_eq!(flow.qualy()[0].car_id, 2);
        assert_eq!(flow.qualy()[0].grid_pos, 1);
        assert_eq!(flow.qualy()[1].car_id, 1);
        assert_eq!(flow.qualy()[2].car_id, 3);
        assert_eq!(flow.qualy()[2].grid_pos, 3);
    }

    #[test]
    fn qualifying_laps_ignored_when_not_qualifying() {
        let mut flow = SessionFlow::new();
        flow.start_qualifying(60.0, vec![QualyEntry::new(1, "a")]);
        flow.report_qualy_lap(1, 0.5); // non-finite/low times rejected
        flow.report_qualy_lap(1, f32::NAN);
        assert_eq!(flow.qualy()[0].best_lap, 1e9);
        flow.tick(60.0);
        flow.report_qualy_lap(1, 70.0); // phase is now GridPublished
        assert_eq!(flow.qualy()[0].best_lap, 1e9);
    }

    #[test]
    fn grid_waits_two_seconds_before_countdown() {
        let mut flow = SessionFlow::new();
        flow.publish_grid();
        assert_eq!(flow.phase(), SessionFlowPhase::GridPublished);
        flow.tick(1.0);
        assert_eq!(flow.phase(), SessionFlowPhase::GridPublished);
        flow.tick(1.5);
        assert_eq!(flow.phase(), SessionFlowPhase::Countdown);
        assert_eq!(flow.countdown(), 5.0);
    }

    #[test]
    fn countdown_reaches_green() {
        let mut flow = SessionFlow::new();
        flow.start_race_direct(3, 5.0);
        assert_eq!(flow.phase(), SessionFlowPhase::Countdown);
        flow.tick(3.0);
        assert_eq!(flow.countdown(), 2.0);
        flow.tick(2.0);
        assert!(flow.is_green());
        assert!(!flow.blocks_drive());
        assert_eq!(flow.total_laps(), 3);
    }

    #[test]
    fn direct_race_clamps_inputs() {
        let mut flow = SessionFlow::new();
        flow.start_race_direct(0, 0.0);
        assert_eq!(flow.total_laps(), 1);
        assert_eq!(flow.countdown(), 1.0);
    }

    #[test]
    fn finish_race_sorts_by_finished_laps_time() {
        let mut flow = SessionFlow::new();
        let mut winner = RaceResultEntry::new(1, "winner");
        winner.finished = true;
        winner.laps = 10;
        winner.total_time = 600.0;
        let mut runner_up = RaceResultEntry::new(2, "runner");
        runner_up.finished = true;
        runner_up.laps = 10;
        runner_up.total_time = 601.0;
        let mut dnf = RaceResultEntry::new(3, "dnf");
        dnf.dnf = true;

        flow.finish_race(vec![dnf, runner_up, winner]);
        assert_eq!(flow.phase(), SessionFlowPhase::Finished);
        assert_eq!(flow.results()[0].car_id, 1);
        assert_eq!(flow.results()[0].position, 1);
        assert_eq!(flow.results()[1].car_id, 2);
        assert_eq!(flow.results()[2].car_id, 3);
    }

    #[test]
    fn more_laps_beats_finished_only() {
        let mut flow = SessionFlow::new();
        let mut a = RaceResultEntry::new(1, "a");
        a.finished = true;
        a.laps = 9;
        a.total_time = 500.0;
        let mut b = RaceResultEntry::new(2, "b");
        b.finished = true;
        b.laps = 10;
        b.total_time = 600.0;
        flow.finish_race(vec![a, b]);
        assert_eq!(flow.results()[0].car_id, 2);
    }

    #[test]
    fn idle_and_finished_are_terminal_for_tick() {
        let mut flow = SessionFlow::new();
        flow.tick(10.0);
        assert_eq!(flow.phase(), SessionFlowPhase::Idle);
        assert_eq!(flow.phase_time(), 0.0);

        flow.start_race_direct(1, 1.0);
        flow.tick(1.0);
        flow.finish_race(Vec::new());
        flow.tick(10.0);
        assert_eq!(flow.phase(), SessionFlowPhase::Finished);
    }

    #[test]
    fn reset_returns_to_idle() {
        let mut flow = SessionFlow::new();
        flow.start_qualifying(60.0, vec![QualyEntry::new(1, "a")]);
        flow.reset();
        assert_eq!(flow.phase(), SessionFlowPhase::Idle);
        assert!(flow.qualy().is_empty());
    }
}
