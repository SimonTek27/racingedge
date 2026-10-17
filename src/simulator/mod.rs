//! RacingEdge — runtime simulatore (fisica, cockpit, sessione).

pub mod ac_import;
pub mod aero;
pub mod brakes;
pub mod car_data;
pub mod car_sound;
pub mod cockpit;
pub mod drivetrain;
pub mod electronics;
pub mod engine;
pub mod feature_hub;
pub mod fonts;
pub mod lut;
pub mod personal_best;
pub mod physics;
pub mod rebank;
pub mod session;
pub mod session_flow;
pub mod strategy;
pub mod suspension;
pub mod timing;
pub mod tire;
pub mod track_limits;
pub mod user_stats;

pub use ac_import::{load_car_auto, AcImporter};
pub use car_data::CarPhysicsData;
pub use car_sound::CarSoundPlugin;
pub use cockpit::{CockpitPlugin, CockpitTelemetry};
pub use feature_hub::FeatureHub;
pub use personal_best::{PersonalBestRecord, PersonalBestStore};
pub use physics::{integrate_transform, step_vehicle, VehicleConfig, VehicleState};
pub use rebank::{ensure_default_rebank, RebankBuilder};
pub use session::{defaults_for_mode, GameSessionMode, RaceFlag, RaceSession, SessionStartParams};
pub use session_flow::{SessionFlow, SessionFlowPhase};
pub use strategy::{PitStopRecommendation, StrategyConfig, StrategySimulator, StrategyState};
pub use timing::{LapSectorTimer, LapTimer, LapTimerEstimate};
pub use track_limits::{
    TrackLimitsConfig, TrackLimitsMonitor, TrackLimitsPenalty, TrackLimitsViolation,
};
pub use user_stats::DriverStats;
