//! Renzora runtime plugin for racingedge.
//!
//! This crate lives in the racingedge repository but builds inside the Renzora
//! workspace (`engine/crates/racingedge_game/`), where its `bevy` and `renzora`
//! dependencies resolve. See `docs/RENZORA_MIGRATION.md` for the staged plan,
//! exactly what to copy where, and what is still missing.
//!
//! [`RacingEdgePlugin`] is a Renzora **runtime** plugin: the engine is Bevy, so
//! a compiled-in plugin is an ordinary [`Plugin`] that adds the game layer.
//! `renzora::add!` is what `cargo renzora sync` reads to link this crate into
//! the runtime binary and list it in the generated `plugins.rs`; the macro
//! itself only type-checks that this really is a `Plugin + Default`.

use renzora::bevy::prelude::*;

use racingedge::plugins::RacingEdgeGamePlugin;

/// The whole racingedge game layer — vehicle physics, scene, camera, HUD and
/// the audio mix models — as one Renzora runtime plugin.
#[derive(Default)]
pub struct RacingEdgePlugin;

impl Plugin for RacingEdgePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(RacingEdgeGamePlugin);
    }
}

renzora::add!(RacingEdgePlugin, Runtime);
