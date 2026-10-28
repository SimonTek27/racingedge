//! Game-side assembly: the scene, the chase camera and the HUD that turn the
//! simulator modules into something you can actually drive.
//!
//! The model code lives in [`crate::simulator`] (camera controller, HUD, cockpit
//! instruments, vehicle physics); this plugin only wires it to the Bevy app so
//! `src/main.rs` stays a plugin list.

use bevy::prelude::*;

use crate::simulator::camera::{CameraController, CameraMode};
use crate::simulator::car_sound::{
    CameraMode as SoundCameraMode, CarSound, CarSoundPlugin, CommonSound,
    TrackSurface as SoundSurface,
};
use crate::simulator::cockpit::CockpitPlugin;
use crate::simulator::hud::{spawn_race_hud, HudPlugin};
use crate::simulator::physics::VehicleState;

/// The simulator's camera controller, as a Bevy resource.
#[derive(Resource, Default)]
pub struct DriveCamera(pub CameraController);

/// Plugin that spawns the world, follows the car with the shared
/// [`CameraController`] and installs the HUD.
pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DriveCamera>()
            .add_plugins((HudPlugin, CockpitPlugin, CarSoundPlugin))
            .add_systems(Startup, (spawn_scene, spawn_hud))
            .add_systems(
                Update,
                (cycle_camera_mode, follow_car, drive_car_audio).chain(),
            );
    }
}

fn spawn_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        DirectionalLight {
            illuminance: 10_000.0,
            ..default()
        },
        Transform::from_xyz(50.0, 80.0, 30.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));

    // Stand-in for a track surface until the track loader lands.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(400.0, 0.1, 400.0))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.22, 0.28, 0.22),
            perceptual_roughness: 0.95,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.05, 0.0),
    ));

    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
}

fn spawn_hud(mut commands: Commands) {
    spawn_race_hud(&mut commands);
}

/// Feed the audio models from the driven car.
///
/// The engine mix already follows the car through `car_sound::update_car_sound`
/// (rpm, throttle, gear, speed); this system supplies the two inputs it cannot
/// see: the cockpit/exterior split from the active camera, and the tyre/surface
/// layers, which had no caller at all.
fn drive_car_audio(
    camera: Res<DriveCamera>,
    mut sound: ResMut<CarSound>,
    mut common: ResMut<CommonSound>,
    cars: Query<&VehicleState>,
) {
    sound.camera = sound_camera_mode(camera.0.mode());

    let Ok(car) = cars.single() else {
        return;
    };
    // The stand-in world has no grip grid yet, so the rolling and skid layers
    // are asphalt driven by speed and lateral slide; the track loader replaces
    // both once it lands.
    let speed = car.speed.abs();
    let slip = (car.lat_vel.abs() / 6.0).clamp(0.0, 1.0);
    common.set_surface(SoundSurface::Asphalt, speed, slip);
}

fn sound_camera_mode(mode: CameraMode) -> SoundCameraMode {
    match mode {
        CameraMode::Cockpit => SoundCameraMode::Cockpit,
        CameraMode::Chase => SoundCameraMode::Chase,
        CameraMode::Free => SoundCameraMode::Free,
    }
}

/// Toggle cockpit ⇄ chase with `C`.
fn cycle_camera_mode(keys: Res<ButtonInput<KeyCode>>, mut camera: ResMut<DriveCamera>) {
    if !keys.just_pressed(KeyCode::KeyC) {
        return;
    }
    let next = match camera.0.mode() {
        CameraMode::Cockpit => CameraMode::Chase,
        _ => CameraMode::Cockpit,
    };
    camera.0.set_mode(next);
}

/// Drive the Bevy camera from the simulator controller, so cockpit head-bob and
/// chase lag behave exactly as the ported model says.
fn follow_car(
    time: Res<Time>,
    mut camera: ResMut<DriveCamera>,
    car: Query<(&Transform, &VehicleState), Without<Camera3d>>,
    mut view: Query<&mut Transform, With<Camera3d>>,
) {
    let Ok((car_transform, car_state)) = car.single() else {
        return;
    };
    let Ok(mut camera_transform) = view.single_mut() else {
        return;
    };

    let body = Mat4::from_rotation_translation(car_transform.rotation, car_transform.translation);
    camera
        .0
        .update(time.delta_secs(), body, car_state.speed.abs() * 3.6);

    let eye = camera.0.position();
    let forward = camera.0.forward();
    if eye.is_finite() && forward.is_finite() && forward.length_squared() > 1e-6 {
        *camera_transform = Transform::from_translation(eye).looking_at(eye + forward, Vec3::Y);
    }
}
