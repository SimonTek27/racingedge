use bevy::prelude::*;

use racingedge::editor::audio::AudioPlugin;
use racingedge::plugins::RacingEdgeGamePlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((RacingEdgeGamePlugin, AudioPlugin))
        .run();
}
