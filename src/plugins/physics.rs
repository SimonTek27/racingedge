//! Game-side vehicle physics: the simulator's [`VehicleState`] driven on the
//! deterministic 1 kHz [`SimulationLoop`].
//!
//! This is the app-layer half of the physics: [`crate::simulator::physics`]
//! owns the model (`step_vehicle`), this plugin owns the frame loop, the
//! keyboard binding and the [`Transform`] integration, so the game binary and
//! any future host drive exactly the same code path.

use bevy::prelude::*;

use crate::simulator::drivetrain::GearboxModel;
use crate::simulator::physics::{integrate_transform, step_vehicle, VehicleConfig, VehicleState};
use crate::simulator::r#loop::{SimulationLoop, PHYSICS_DT};

/// Accelerator keys.
pub const THROTTLE_KEYS: [KeyCode; 2] = [KeyCode::KeyW, KeyCode::ArrowUp];
/// Brake keys.
pub const BRAKE_KEYS: [KeyCode; 2] = [KeyCode::KeyS, KeyCode::ArrowDown];
/// Steer-left keys.
pub const STEER_LEFT_KEYS: [KeyCode; 2] = [KeyCode::KeyA, KeyCode::ArrowLeft];
/// Steer-right keys.
pub const STEER_RIGHT_KEYS: [KeyCode; 2] = [KeyCode::KeyD, KeyCode::ArrowRight];

/// Upper bound on the physics steps a single frame may run, so a long stall
/// (window drag, breakpoint, …) cannot turn into a multi-second catch-up.
pub const MAX_STEPS_PER_FRAME: u32 = 100;

/// Automatic gearbox: [`GearboxModel`] configured from the [`VehicleConfig`]
/// ratios, so the shift points follow the car's own gearing.
#[derive(Resource)]
pub struct AutoGearbox(GearboxModel);

impl FromWorld for AutoGearbox {
    fn from_world(world: &mut World) -> Self {
        let config = world.resource::<VehicleConfig>();
        let mut gearbox = GearboxModel::new();
        gearbox.set_ratios(
            config.drivetrain.gear_ratios.clone(),
            config.drivetrain.final_drive,
        );
        Self(gearbox)
    }
}

impl AutoGearbox {
    /// The current gear (`1..=ratios`, `0` = neutral).
    pub fn gear(&self) -> i8 {
        self.0.gear() as i8
    }
}

/// Plugin that spawns the car and steps it with the simulator physics.
pub struct PhysicsPlugin;

impl Plugin for PhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ClearColor(Color::srgb(0.02, 0.02, 0.04)))
            .init_resource::<VehicleConfig>()
            .init_resource::<SimulationLoop>()
            .init_resource::<AutoGearbox>()
            .add_systems(Startup, spawn_car)
            .add_systems(Update, (read_drive_input, step_drive).chain());
    }
}

fn spawn_car(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(1.8, 1.1, 4.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.82, 0.16, 0.14),
            perceptual_roughness: 0.45,
            ..default()
        })),
        Transform::from_xyz(0.0, 0.55, 0.0),
        VehicleState::default(),
    ));
}

fn any_pressed(keys: &ButtonInput<KeyCode>, codes: &[KeyCode]) -> bool {
    codes.iter().any(|code| keys.pressed(*code))
}

/// Sample the keyboard into the vehicle inputs. Digital keys are full-scale;
/// the steering ramp in `step_vehicle` turns them into a progressive lock.
fn read_drive_input(keys: Res<ButtonInput<KeyCode>>, mut cars: Query<&mut VehicleState>) {
    let throttle = f32::from(u8::from(any_pressed(&keys, &THROTTLE_KEYS)));
    let brake = f32::from(u8::from(any_pressed(&keys, &BRAKE_KEYS)));
    let left = f32::from(u8::from(any_pressed(&keys, &STEER_LEFT_KEYS)));
    let right = f32::from(u8::from(any_pressed(&keys, &STEER_RIGHT_KEYS)));

    for mut car in &mut cars {
        car.throttle = throttle;
        car.brake = brake;
        car.steer_input = right - left;
    }
}

/// Advance the vehicle by whole `PHYSICS_DT` steps and write the result back to
/// the entity's [`Transform`].
fn step_drive(
    time: Res<Time>,
    config: Res<VehicleConfig>,
    mut sim_loop: ResMut<SimulationLoop>,
    mut gearbox: ResMut<AutoGearbox>,
    mut cars: Query<(&mut VehicleState, &mut Transform)>,
) {
    let steps = sim_loop
        .tick(time.delta_secs() as f64)
        .min(MAX_STEPS_PER_FRAME);
    if steps == 0 {
        return;
    }
    let dt = PHYSICS_DT as f32;

    for (mut car, mut transform) in &mut cars {
        for _ in 0..steps {
            step_vehicle(&config, &mut car, dt);
            gearbox.0.auto_shift(
                car.engine.rpm,
                config.engine.rpm_limiter,
                config.engine.rpm_idle,
            );
            car.drivetrain.gear = gearbox.gear();
        }
        integrate_transform(&car, &mut transform, dt * steps as f32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use std::time::Duration;

    #[test]
    fn throttle_moves_the_car_forward() {
        let mut world = World::new();
        world.insert_resource(VehicleConfig::default());
        world.insert_resource(SimulationLoop::default());
        world.init_resource::<AutoGearbox>();

        let mut time = Time::<()>::default();
        time.advance_by(Duration::from_millis(20));
        world.insert_resource(time);

        let car = world
            .spawn((
                VehicleState {
                    throttle: 1.0,
                    ..default()
                },
                Transform::default(),
            ))
            .id();

        world.run_system_once(step_drive).expect("step_drive");

        let transform = world.get::<Transform>(car).expect("car transform");
        // `integrate_transform` drives the car along -Z while yaw is 0.
        assert!(
            transform.translation.z < 0.0,
            "z = {}",
            transform.translation.z
        );
    }
}
