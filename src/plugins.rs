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

use bevy::prelude::*;

pub mod game;
pub mod physics;

pub use game::GamePlugin;
pub use physics::PhysicsPlugin;

/// The complete game layer: vehicle physics, scene, camera, HUD and audio.
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
        app.add_plugins((PhysicsPlugin, GamePlugin));
    }
}
