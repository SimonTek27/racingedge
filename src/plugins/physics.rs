use bevy::prelude::*;

/// Component that holds a simple state for a car.
#[derive(Component, Default)]
pub struct Car {
    /// Current forward speed (m/s).
    pub speed: f32,
    /// Rotation yaw in radians.
    pub yaw: f32,
}

/// Plugin that inserts the Car component and a simple physics/rotation system.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::rgb(0.1, 0.1, 0.1)))
            .add_systems(Startup, spawn_car)
            .add_systems(Update, update_physics);
    }
}

fn spawn_car(mut commands: Commands) {
    // Spawn a default PbrBundle (a white cube) and insert our Car component.
    commands.spawn(PbrBundle::default()).insert(Car::default());
}

fn update_physics(time: Res<Time>, mut query: Query<(&mut Transform, &mut Car)>) {
    for (mut transform, mut car) in &mut query {
        // Very naive "physics": accelerate forward and yaw slowly.
        car.speed += 0.5 * time.delta_seconds();
        car.yaw += 0.2 * time.delta_seconds();

        // Move forward based on yaw.
        let forward = Vec3::new(car.yaw.cos(), 0.0, car.yaw.sin());
        transform.translation += forward * car.speed * time.delta_seconds();

        // Apply yaw rotation.
        transform.rotation = Quat::from_axis_angle(Vec3::Y, car.yaw);
    }
}