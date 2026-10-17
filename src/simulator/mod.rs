//! RacingEdge — runtime simulatore (fisica, cockpit, sessione).

pub mod ac_import;
pub mod aero;
pub mod aero_draft;
pub mod brake_thermal;
pub mod brakes;
pub mod camera;
pub mod car_data;
pub mod car_sound;
pub mod cockpit;
pub mod damage;
pub mod drivetrain;
pub mod electronics;
pub mod engine;
pub mod feature_hub;
pub mod ffb;
pub mod fonts;
pub mod input;
pub mod lut;
pub mod personal_best;
pub mod physics;
pub mod pit;
pub mod rebank;
pub mod session;
pub mod session_flow;
pub mod setup;
pub mod strategy;
pub mod suspension;
pub mod timing;
pub mod tire;
pub mod tire_wear;
pub mod track_limits;
pub mod user_stats;

pub use ac_import::{load_car_auto, AcImporter};
pub use aero_draft::{compute_draft, compute_draft_with_width, DraftState};
pub use brake_thermal::{
    BrakeDiscWearState, BrakeFadeState, BrakeFadeType, BrakePadWearState, BrakeThermalConfig,
    BrakeThermalModel, BrakeThermalState, BrakeWearData, BrakeWearSystem,
};
pub use camera::{CameraController, CameraMode};
pub use car_data::CarPhysicsData;
pub use car_sound::CarSoundPlugin;
pub use cockpit::{CockpitPlugin, CockpitTelemetry};
pub use damage::{
    apply_mechanical_impact, apply_mechanical_wear, apply_pit_contact_damage, apply_rf2_impulse,
    impulse_from_impact, sample_damage, sample_mechanical_effects, CollisionEvent, DamageConfig,
    DamageSystem, DamageTelemSample, DamageType, DamageZone, MechEffectMultipliers, MechTelemetry,
    MechWearConfig, RepairData, Rf2DamageParams,
};
pub use drivetrain::{DiffConfig, DiffState, DiffType, DifferentialModel, GearboxModel};
pub use feature_hub::FeatureHub;
pub use ffb::{FfbActuator, FfbBridge, FfbInputs, FfbOutput, FfbSample};
pub use input::{
    key_from_name, key_name, load_keyboard_mapping, save_keyboard_mapping, ControllerBackend,
    ControllerState, InputManager, JoystickMapping, KeyboardMapping, AXIS_COUNT,
};
pub use personal_best::{PersonalBestRecord, PersonalBestStore};
pub use physics::{integrate_transform, step_vehicle, VehicleConfig, VehicleState};
pub use pit::{
    apply_pit_collision_results, apply_strategy_to_repair_input,
    integrate_pit_queue_with_garage_exit, is_exit_path_blocked, sync_pit_collision_from_queue,
    GarageBlockReason, GarageBoxPose, GarageExitConfig, GarageExitController, GarageExitInput,
    GarageExitOutput, GarageExitPhase, PitAxis, PitCarBody, PitCollisionContact, PitJob,
    PitJobState, PitJobType, PitLaneCollision, PitLaneCollisionConfig, PitLaneQueue,
    PitLaneQueueConfig, PitLaneRepair, PitQueueEntry, PitQueueRole, PitQueueStatus,
    PitRepairConfig, PitRepairInput, PitRepairOutput, PitStrategyState,
};
pub use rebank::{ensure_default_rebank, RebankBuilder};
pub use session::{defaults_for_mode, GameSessionMode, RaceFlag, RaceSession, SessionStartParams};
pub use session_flow::{SessionFlow, SessionFlowPhase};
pub use setup::{
    load_setup, save_setup, setup_path, GarageRow, GarageTab, SetupData, SetupField, SetupGarage,
    SetupParams,
};
pub use strategy::{PitStopRecommendation, StrategyConfig, StrategySimulator, StrategyState};
pub use timing::{LapSectorTimer, LapTimer, LapTimerEstimate};
pub use tire_wear::{
    TireCompound, TireCompoundData, TireFlatSpot, TireFlatSpotState, TireThermalConfig,
    TireThermalState, TireWearConfig, TireWearDetailState, TireWearSystem,
};
pub use track_limits::{
    TrackLimitsConfig, TrackLimitsMonitor, TrackLimitsPenalty, TrackLimitsViolation,
};
pub use user_stats::DriverStats;
