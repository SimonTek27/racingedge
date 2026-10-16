use bevy::prelude::*;

use racingedge::editor::audio::AudioPlugin;
use racingedge::plugins::physics::PhysicsPlugin;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(bevy::pbr::PbrPlugin {
            prepass_enabled: true,
        }))
        .add_plugins((PhysicsPlugin, AudioPlugin))
        .insert_resource(ClearColor(Color::rgb(0.1, 0.1, 0.1)))
        .run();
}
