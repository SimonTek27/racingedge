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
pub mod lut;
pub mod physics;
pub mod rebank;
pub mod suspension;
pub mod tire;

pub use ac_import::{load_car_auto, AcImporter};
pub use car_data::CarPhysicsData;
pub use car_sound::CarSoundPlugin;
pub use cockpit::{CockpitPlugin, CockpitTelemetry};
pub use physics::{integrate_transform, step_vehicle, VehicleConfig, VehicleState};
pub use rebank::{ensure_default_rebank, RebankBuilder};
