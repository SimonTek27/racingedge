//! Race strategy and pit-stop planning — port of ksengine
//! `engine/physics/StrategySimulator.*` (GPL-3.0).
//! Original: <https://github.com/SimonTek27/ksengine>
//!
//! This is the fuel/pit half of the backlog's B10 item. ksengine has no
//! separate `FuelSystem`; fuel planning lives entirely in `StrategySimulator`,
//! so the port keeps it together instead of inventing a `fuel.rs`.

/// Strategy configuration (port of `StrategyConfig`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrategyConfig {
    /// Total laps of the race.
    pub total_laps: i32,
    /// Nominal fuel burn per lap, in litres.
    pub fuel_per_lap: f32,
    /// Nominal tyre wear per lap, as a fraction of life.
    pub tire_degradation_rate: f32,
    /// Time lost by an extra pit stop, in seconds.
    pub pit_stop_time_loss: f32,
    /// Weight given to track position when planning.
    pub track_position_weight: f32,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            total_laps: 50,
            fuel_per_lap: 1.5,
            tire_degradation_rate: 0.02,
            pit_stop_time_loss: 22.0,
            track_position_weight: 0.7,
        }
    }
}

/// Live strategy state fed into the simulator (port of `StrategyState`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StrategyState {
    /// Current lap number (0-based, as in the reference).
    pub current_lap: i32,
    /// Current race position (1 = leader).
    pub position: i32,
    /// Fuel currently in the tank, in litres.
    pub current_fuel: f32,
    /// Observed fuel burn per lap, in litres.
    pub fuel_per_lap: f32,
    /// Tyre wear per corner, `0..1`.
    pub tire_wear: [f32; 4],
    /// Last lap time in seconds.
    pub last_lap_time: f32,
    /// Best lap time in seconds (`1e9` when none has been set).
    pub best_lap_time: f32,
    /// Gap to the car ahead, in seconds.
    pub gap_ahead: f32,
    /// Gap to the car behind, in seconds.
    pub gap_behind: f32,
}

impl Default for StrategyState {
    fn default() -> Self {
        Self {
            current_lap: 0,
            position: 1,
            current_fuel: 60.0,
            fuel_per_lap: 1.5,
            tire_wear: [0.0; 4],
            last_lap_time: 0.0,
            best_lap_time: 1e9,
            gap_ahead: 0.0,
            gap_behind: 0.0,
        }
    }
}

/// A pit-stop recommendation produced by [`StrategySimulator`].
#[derive(Clone, Debug, PartialEq)]
pub struct PitStopRecommendation {
    /// Whether the driver should pit.
    pub should_pit: bool,
    /// Lap to pit on (`-1` when none).
    pub pit_lap: i32,
    /// Estimated time gain (negative means the stop costs time).
    pub estimated_time_gain: f32,
    /// Fuel to add, in litres.
    pub fuel_to_add: f32,
    /// Whether to change tyres during the stop.
    pub change_tires: bool,
    /// Human-readable reason for the recommendation.
    pub reason: String,
}

impl Default for PitStopRecommendation {
    fn default() -> Self {
        Self {
            should_pit: false,
            pit_lap: -1,
            estimated_time_gain: 0.0,
            fuel_to_add: 0.0,
            change_tires: true,
            reason: String::new(),
        }
    }
}

/// Fuel and pit-stop planner (port of `StrategySimulator`).
#[derive(Clone, Debug, Default)]
pub struct StrategySimulator {
    config: StrategyConfig,
    state: StrategyState,
}

impl StrategySimulator {
    /// A planner with default config and state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the strategy configuration.
    pub fn set_strategy_config(&mut self, config: StrategyConfig) {
        self.config = config;
    }

    /// The current strategy configuration.
    pub fn strategy_config(&self) -> StrategyConfig {
        self.config
    }

    /// A copy of the last state fed through [`update`](Self::update).
    pub fn strategy_state(&self) -> StrategyState {
        self.state
    }

    /// Feeds the current race state into the planner. `dt` is unused by the
    /// reference implementation and kept for call-site parity.
    pub fn update(&mut self, _dt: f64, current_state: StrategyState) {
        self.state = current_state;
    }

    /// Decides whether to pit, and for what.
    ///
    /// Recommends a stop when fewer than three laps of fuel remain or the
    /// average tyre wear exceeds `0.85`.
    pub fn calculate_pit_stop_recommendation(&self) -> PitStopRecommendation {
        let mut recommendation = PitStopRecommendation::default();

        let avg_wear = self.state.tire_wear.iter().sum::<f32>() * 0.25;
        let laps_left = (self.config.total_laps - self.state.current_lap) as f32;
        let fuel_per_lap = if self.state.fuel_per_lap > 1e-6 {
            self.state.fuel_per_lap
        } else {
            1.5
        };
        let fuel_laps = self.state.current_fuel / fuel_per_lap;

        if fuel_laps < 3.0 || avg_wear > 0.85 {
            recommendation.should_pit = true;
            recommendation.pit_lap = self.state.current_lap + 1;
            recommendation.fuel_to_add = self.config.fuel_per_lap * laps_left;
            recommendation.change_tires = avg_wear > 0.6;
            recommendation.reason = if avg_wear > 0.85 {
                "Tire wear critical".to_string()
            } else {
                "Fuel low".to_string()
            };
            recommendation.estimated_time_gain = -self.config.pit_stop_time_loss;
        }

        recommendation
    }

    /// Fuel needed for `laps_remaining`, with a 5% safety margin.
    pub fn calculate_optimal_fuel_load(&self, laps_remaining: i32) -> f32 {
        self.config.fuel_per_lap * laps_remaining as f32 * 1.05
    }

    /// Remaining tyre life, `1 - max wear`.
    pub fn calculate_tire_life(&self) -> f32 {
        1.0 - self.state.tire_wear.iter().copied().fold(0.0, f32::max)
    }

    /// Current lap from the last [`update`](Self::update).
    pub fn current_lap(&self) -> i32 {
        self.state.current_lap
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_reference() {
        let config = StrategyConfig::default();
        assert_eq!(config.total_laps, 50);
        assert_eq!(config.fuel_per_lap, 1.5);
        assert_eq!(config.pit_stop_time_loss, 22.0);

        let state = StrategyState::default();
        assert_eq!(state.position, 1);
        assert_eq!(state.current_fuel, 60.0);
        assert_eq!(state.best_lap_time, 1e9);
    }

    #[test]
    fn no_stop_when_fuel_and_tires_are_fine() {
        let mut sim = StrategySimulator::new();
        sim.update(0.0, StrategyState::default());
        let recommendation = sim.calculate_pit_stop_recommendation();
        assert!(!recommendation.should_pit);
        assert_eq!(recommendation.pit_lap, -1);
    }

    #[test]
    fn recommends_stop_on_low_fuel() {
        let mut sim = StrategySimulator::new();
        sim.update(
            0.0,
            StrategyState {
                current_lap: 10,
                current_fuel: 3.0, // 2 laps left
                ..Default::default()
            },
        );
        let recommendation = sim.calculate_pit_stop_recommendation();
        assert!(recommendation.should_pit);
        assert_eq!(recommendation.pit_lap, 11);
        assert_eq!(recommendation.reason, "Fuel low");
        assert_eq!(recommendation.fuel_to_add, 1.5 * 40.0);
        assert_eq!(recommendation.estimated_time_gain, -22.0);
        assert!(!recommendation.change_tires);
    }

    #[test]
    fn recommends_stop_on_critical_wear_and_changes_tires() {
        let mut sim = StrategySimulator::new();
        sim.update(
            0.0,
            StrategyState {
                tire_wear: [0.9, 0.9, 0.9, 0.9],
                ..Default::default()
            },
        );
        let recommendation = sim.calculate_pit_stop_recommendation();
        assert!(recommendation.should_pit);
        assert_eq!(recommendation.reason, "Tire wear critical");
        assert!(recommendation.change_tires);
    }

    #[test]
    fn optimal_fuel_load_includes_margin() {
        let sim = StrategySimulator::new();
        assert!((sim.calculate_optimal_fuel_load(20) - 1.5 * 20.0 * 1.05).abs() < 1e-6);
    }

    #[test]
    fn tire_life_tracks_max_wear() {
        let mut sim = StrategySimulator::new();
        sim.update(
            0.0,
            StrategyState {
                tire_wear: [0.1, 0.2, 0.3, 0.4],
                ..Default::default()
            },
        );
        assert!((sim.calculate_tire_life() - 0.6).abs() < 1e-6);
        assert_eq!(sim.current_lap(), 0);
    }

    #[test]
    fn strategy_config_round_trips() {
        let mut sim = StrategySimulator::new();
        let config = StrategyConfig {
            total_laps: 30,
            fuel_per_lap: 2.0,
            ..Default::default()
        };
        sim.set_strategy_config(config);
        assert_eq!(sim.strategy_config(), config);
    }
}
