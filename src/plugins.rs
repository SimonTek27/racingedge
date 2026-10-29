//! Game-side Bevy plugins.
//!
//! Content editing (including the audio editor and its in-game bank browser)
//! lives in [`crate::editor`].
//!
//! [`RacingEdgeGamePlugin`] is the whole game layer behind one plugin, so a host
//! — the `racingedge` binary today, the Renzora wrapper in `renzora/` next — adds
//! one plugin instead of remembering the composition. The parts stay public on
//! their own: [`physics`] owns the fixed-step vehicle loop, [`game`] owns the
//! scene, camera, HUD and audio wiring.
//!
//! Two seams let a host keep its own engine services: [`DriveControls`] carries
//! the driving input as plain data, and `simulator::audio_host::CarMix` carries
//! the car mix, so neither the input plugin nor the mixer has to be ours.

use bevy::prelude::*;

pub mod game;
pub mod keymap;
pub mod menu;
pub mod physics;

pub use game::GamePlugin;
pub use menu::{MenuPlugin, MenuState};
pub use physics::{DriveBindings, DriveControls, KeyboardDriveInput, PhysicsPlugin};

// The input seam: a host writes [`DriveControls`] from its own input plugin
// (Renzora's, or a wheel) after taking the keyboard sampler out of the way with
// [`KeyboardDriveInput(false)`](KeyboardDriveInput). See
// `docs/RENZORA_MIGRATION.md`.

/// Holds the simulation while a menu is open.
///
/// The menu sets it ([`MenuPlugin`]) and the drive loop reads it
/// ([`PhysicsPlugin`]), so neither has to know about the other.
#[derive(Resource, Default, Clone, Copy, Debug)]
pub struct SimulationPause(pub bool);

impl SimulationPause {
    /// `true` while the simulation must not step.
    pub fn paused(&self) -> bool {
        self.0
    }
}

/// The complete game layer: vehicle physics, scene, camera, HUD, audio and the
/// pause/settings menus.
///
/// The host still owns the engine plugins (`DefaultPlugins`, or Renzora's own
/// set), the asset/audio plugins that pair with them, and the runner. That is
/// the whole contract a Renzora plugin needs — see `docs/RENZORA_MIGRATION.md`.
///
/// # Example
///
/// ```no_run
/// use bevy::prelude::*;
/// use racingedge::plugins::RacingEdgeGamePlugin;
///
/// App::new()
///     .add_plugins(DefaultPlugins)
///     .add_plugins(RacingEdgeGamePlugin)
///     .run();
/// ```
#[derive(Default)]
pub struct RacingEdgeGamePlugin;

impl Plugin for RacingEdgeGamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((PhysicsPlugin, GamePlugin, MenuPlugin));
    }
}
