//! Track-limits detection → warnings → penalties — port of ksengine
//! `src/simulator/TrackLimitsMonitor.h` (GPL-3.0).
//! Original: <https://github.com/SimonTek27/ksengine>
//!
//! The monitor watches each car's distance from the track centerline (or a
//! simple on-track hint when no centerline is loaded) and, after a grace
//! period, reports a violation. Reports escalate to time, drive-through,
//! stop-go and disqualification penalties.
//!
//! The reference `update` takes a `RaceSessionManager*` and calls
//! `addPenalty` on it. `RaceSessionManager` is not ported yet, so
//! [`TrackLimitsMonitor::update`] instead **returns** the violation and the
//! escalation penalty it implies; the caller applies it to whatever session
//! type it owns. This keeps the monitor free of the session manager while
//! preserving the exact escalation thresholds and ordering.

use bevy::math::Vec3;

/// Track-limits thresholds (port of `TrackLimitsConfig`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackLimitsConfig {
    /// Lateral distance from the centerline beyond which the car is off track.
    pub max_lateral_m: f32,
    /// How long the car may be off track before a violation is reported.
    pub grace_sec: f32,
    /// Minimum time between two reports for the same car.
    pub cooldown_sec: f32,
    /// Whether the monitor runs at all.
    pub enabled: bool,
    /// Report cadence hint (kept for parity; the reference does not use it).
    pub warn_every: i32,
    /// Report count at which a time penalty is applied (and every multiple).
    pub time_penalty_at: i32,
    /// Seconds added by a time penalty.
    pub time_penalty_sec: f32,
    /// Report count at which a drive-through is applied.
    pub drive_through_at: i32,
    /// Report count at which a stop-go is applied.
    pub stop_go_at: i32,
    /// Report count at which the car is disqualified (`0` disables).
    pub dq_at: i32,
}

impl Default for TrackLimitsConfig {
    fn default() -> Self {
        Self {
            max_lateral_m: 8.0,
            grace_sec: 0.35,
            cooldown_sec: 1.5,
            enabled: true,
            warn_every: 1,
            time_penalty_at: 3,
            time_penalty_sec: 1.0,
            drive_through_at: 5,
            stop_go_at: 8,
            dq_at: 12,
        }
    }
}

/// The penalty a violation escalates to (port of the `Penalty::Type` values
/// the reference applies).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TrackLimitsPenalty {
    /// Add `f32` seconds to the session time.
    TimeAdded(f32),
    /// Drive-through penalty.
    DriveThrough,
    /// Stop for `f32` seconds.
    StopGo(f32),
    /// Disqualification.
    Disqualification,
}

/// A reported track-limits violation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackLimitsViolation {
    /// Car that went off track.
    pub car_index: i32,
    /// Total reports for that car so far.
    pub reports: i32,
    /// The escalation penalty, if this report crossed a threshold.
    pub penalty: Option<TrackLimitsPenalty>,
}

#[derive(Clone, Copy, Debug, Default)]
struct CarState {
    off_timer: f32,
    cooldown: f32,
    reports: i32,
}

/// Per-car track-limits monitor (port of `TrackLimitsMonitor`).
///
/// # Example
///
/// ```
/// use racingedge::simulator::track_limits::TrackLimitsMonitor;
/// use bevy::math::Vec3;
/// let mut monitor = TrackLimitsMonitor::default();
/// monitor.set_centerline(vec![
///     Vec3::new(0.0, 0.0, 0.0),
///     Vec3::new(0.0, 0.0, 100.0),
/// ]);
/// // 20 m to the side is well past the 8 m limit.
/// let far = Vec3::new(20.0, 0.0, 50.0);
/// assert!(monitor.update(0.2, 0, far, true).is_none(), "within grace");
/// let violation = monitor.update(0.2, 0, far, true).expect("reports after grace");
/// assert_eq!(violation.reports, 1);
/// assert!(violation.penalty.is_none(), "first report is only a warning");
/// ```
#[derive(Clone, Debug, Default)]
pub struct TrackLimitsMonitor {
    config: TrackLimitsConfig,
    centerline: Vec<Vec3>,
    state: Vec<CarState>,
}

impl TrackLimitsMonitor {
    /// A monitor with the default configuration and no centerline.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the configuration.
    pub fn set_config(&mut self, config: TrackLimitsConfig) {
        self.config = config;
    }

    /// The current configuration.
    pub fn config(&self) -> &TrackLimitsConfig {
        &self.config
    }

    /// Sets the track centerline used for the lateral-distance test.
    pub fn set_centerline(&mut self, points: Vec<Vec3>) {
        self.centerline = points;
    }

    /// The track centerline.
    pub fn centerline(&self) -> &[Vec3] {
        &self.centerline
    }

    /// Advances the monitor for one car.
    ///
    /// Returns a violation when the car has been off track past the grace
    /// period and the cooldown has elapsed; the caller applies
    /// [`TrackLimitsViolation::penalty`] to its session.
    pub fn update(
        &mut self,
        dt: f32,
        car_index: i32,
        pos: Vec3,
        on_track_hint: bool,
    ) -> Option<TrackLimitsViolation> {
        if !self.config.enabled {
            return None;
        }

        let mut off = !on_track_hint;
        if !self.centerline.is_empty() {
            let lateral = self.lateral_distance(pos);
            if lateral > self.config.max_lateral_m {
                off = true;
            } else if on_track_hint {
                off = false;
            }
        }

        let index = car_index.max(0) as usize;
        if self.state.len() <= index {
            self.state.resize(index + 1, CarState::default());
        }

        let mut reported: Option<i32> = None;
        {
            let state = &mut self.state[index];
            state.cooldown = (state.cooldown - dt).max(0.0);
            if off {
                state.off_timer += dt;
                if state.off_timer >= self.config.grace_sec && state.cooldown <= 0.0 {
                    state.cooldown = self.config.cooldown_sec;
                    state.off_timer = 0.0;
                    state.reports += 1;
                    reported = Some(state.reports);
                }
            } else {
                state.off_timer = 0.0;
            }
        }

        reported.map(|reports| TrackLimitsViolation {
            car_index: index as i32,
            reports,
            penalty: self.escalation(reports),
        })
    }

    /// Total reports recorded for `car_index`.
    pub fn reports_for(&self, car_index: i32) -> i32 {
        if car_index < 0 || car_index as usize >= self.state.len() {
            return 0;
        }
        self.state[car_index as usize].reports
    }

    /// Clears the per-car state (port of `resetCar`).
    pub fn reset_car(&mut self, car_index: i32) {
        if car_index >= 0 && (car_index as usize) < self.state.len() {
            self.state[car_index as usize] = CarState::default();
        }
    }

    /// Shortest distance from `pos` to the centerline, in the XZ plane
    /// (port of `lateralDistance`). Returns `0` for a degenerate centerline.
    pub fn lateral_distance(&self, pos: Vec3) -> f32 {
        if self.centerline.len() < 2 {
            return 0.0;
        }
        let mut best = 1e9f32;
        for pair in self.centerline.windows(2) {
            let a = pair[0];
            let b = pair[1];
            let abx = b.x - a.x;
            let abz = b.z - a.z;
            let apx = pos.x - a.x;
            let apz = pos.z - a.z;
            let ab2 = abx * abx + abz * abz;
            let t = if ab2 > 1e-6 {
                ((apx * abx + apz * abz) / ab2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let cx = a.x + abx * t;
            let cz = a.z + abz * t;
            let dx = pos.x - cx;
            let dz = pos.z - cz;
            let d = (dx * dx + dz * dz).sqrt();
            if d < best {
                best = d;
            }
        }
        best
    }

    fn escalation(&self, reports: i32) -> Option<TrackLimitsPenalty> {
        let c = &self.config;
        if c.dq_at > 0 && reports >= c.dq_at {
            return Some(TrackLimitsPenalty::Disqualification);
        }
        if c.stop_go_at > 0 && reports == c.stop_go_at {
            return Some(TrackLimitsPenalty::StopGo(10.0));
        }
        if c.drive_through_at > 0 && reports == c.drive_through_at {
            return Some(TrackLimitsPenalty::DriveThrough);
        }
        if c.time_penalty_at > 0 && reports > 0 && reports % c.time_penalty_at == 0 {
            return Some(TrackLimitsPenalty::TimeAdded(c.time_penalty_sec));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight_centerline() -> Vec<Vec3> {
        vec![Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 0.0, 100.0)]
    }

    #[test]
    fn disabled_monitor_never_reports() {
        let mut monitor = TrackLimitsMonitor::default();
        monitor.set_config(TrackLimitsConfig {
            enabled: false,
            ..Default::default()
        });
        for _ in 0..10 {
            assert!(monitor
                .update(1.0, 0, Vec3::new(50.0, 0.0, 50.0), false)
                .is_none());
        }
    }

    #[test]
    fn within_grace_period_is_ignored() {
        let mut monitor = TrackLimitsMonitor::default();
        assert!(monitor.update(0.2, 0, Vec3::ZERO, false).is_none());
    }

    #[test]
    fn reports_after_grace_period() {
        let mut monitor = TrackLimitsMonitor::default();
        monitor.update(0.2, 0, Vec3::ZERO, false);
        let violation = monitor.update(0.2, 0, Vec3::ZERO, false).expect("report");
        assert_eq!(violation.car_index, 0);
        assert_eq!(violation.reports, 1);
        assert!(
            violation.penalty.is_none(),
            "first report is only a warning"
        );
        assert_eq!(monitor.reports_for(0), 1);
    }

    #[test]
    fn cooldown_blocks_immediate_second_report() {
        let mut monitor = TrackLimitsMonitor::default();
        monitor.update(0.2, 0, Vec3::ZERO, false);
        assert!(monitor.update(0.2, 0, Vec3::ZERO, false).is_some());
        // Still inside the 1.5 s cooldown.
        assert!(monitor.update(0.5, 0, Vec3::ZERO, false).is_none());
        assert_eq!(monitor.reports_for(0), 1);
    }

    #[test]
    fn returning_to_track_resets_the_off_timer() {
        let mut monitor = TrackLimitsMonitor::default();
        monitor.update(0.3, 0, Vec3::ZERO, false);
        assert!(monitor.update(0.1, 0, Vec3::ZERO, true).is_none());
        // Timer was reset; another cold start needs a full grace window first.
        assert!(monitor.update(0.1, 0, Vec3::ZERO, false).is_none());
        assert!(monitor.update(0.3, 0, Vec3::ZERO, false).is_some());
    }

    #[test]
    fn centerline_lateral_distance_marks_off_track() {
        let mut monitor = TrackLimitsMonitor::default();
        monitor.set_centerline(straight_centerline());
        // 20 m away is beyond max_lateral_m (8 m) even with an on-track hint.
        let far = Vec3::new(20.0, 0.0, 50.0);
        monitor.update(0.2, 0, far, true);
        assert!(monitor.update(0.2, 0, far, true).is_some());
        // On the line stays on track.
        assert!(monitor
            .update(0.2, 1, Vec3::new(0.0, 0.0, 50.0), true)
            .is_none());
    }

    #[test]
    fn lateral_distance_matches_projection() {
        let mut monitor = TrackLimitsMonitor::default();
        monitor.set_centerline(straight_centerline());
        assert!((monitor.lateral_distance(Vec3::new(3.0, 5.0, 40.0)) - 3.0).abs() < 1e-4);
        assert!((monitor.lateral_distance(Vec3::new(0.0, 0.0, -10.0)) - 10.0).abs() < 1e-4);
    }

    #[test]
    fn escalation_thresholds_match_reference() {
        let monitor = TrackLimitsMonitor::default();
        assert_eq!(monitor.escalation(1), None);
        assert_eq!(monitor.escalation(2), None);
        assert_eq!(
            monitor.escalation(3),
            Some(TrackLimitsPenalty::TimeAdded(1.0))
        );
        assert_eq!(monitor.escalation(4), None);
        assert_eq!(
            monitor.escalation(5),
            Some(TrackLimitsPenalty::DriveThrough)
        );
        assert_eq!(
            monitor.escalation(6),
            Some(TrackLimitsPenalty::TimeAdded(1.0))
        );
        assert_eq!(
            monitor.escalation(8),
            Some(TrackLimitsPenalty::StopGo(10.0))
        );
        assert_eq!(
            monitor.escalation(12),
            Some(TrackLimitsPenalty::Disqualification)
        );
        assert_eq!(
            monitor.escalation(13),
            Some(TrackLimitsPenalty::Disqualification)
        );
    }

    #[test]
    fn reset_car_clears_reports() {
        let mut monitor = TrackLimitsMonitor::default();
        monitor.update(0.2, 0, Vec3::ZERO, false);
        monitor.update(0.2, 0, Vec3::ZERO, false);
        assert_eq!(monitor.reports_for(0), 1);
        monitor.reset_car(0);
        assert_eq!(monitor.reports_for(0), 0);
        assert_eq!(monitor.reports_for(99), 0);
    }

    #[test]
    fn per_car_state_is_independent() {
        let mut monitor = TrackLimitsMonitor::default();
        monitor.update(0.2, 0, Vec3::ZERO, false);
        monitor.update(0.2, 0, Vec3::ZERO, false);
        monitor.update(0.2, 2, Vec3::ZERO, false);
        monitor.update(0.2, 2, Vec3::ZERO, false);
        assert_eq!(monitor.reports_for(0), 1);
        assert_eq!(monitor.reports_for(1), 0);
        assert_eq!(monitor.reports_for(2), 1);
    }
}
