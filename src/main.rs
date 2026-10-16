use bevy::prelude::*;

use racingedge::editor::audio::AudioPlugin;
use racingedge::plugins::physics::PhysicsPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins((PhysicsPlugin, AudioPlugin))
        .run();
}
