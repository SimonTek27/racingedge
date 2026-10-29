use bevy::prelude::*;

#[cfg(feature = "game-audio")]
use racingedge::editor::audio::AudioPlugin;
use racingedge::plugins::RacingEdgeGamePlugin;

fn main() {
    let mut app = App::new();
    app.add_plugins(DefaultPlugins);
    #[cfg(feature = "game-audio")]
    app.add_plugins(AudioPlugin);
    app.add_plugins(RacingEdgeGamePlugin).run();
}
