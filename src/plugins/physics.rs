//! Game-side vehicle physics: the simulator's [`VehicleState`] driven on the
//! deterministic 1 kHz [`SimulationLoop`].
//!
//! This is the app-layer half of the physics: [`crate::simulator::physics`]
//! owns the model (`step_vehicle`), this plugin owns the frame loop, the
//! keyboard binding and the [`Transform`] integration, so the game binary and
//! any future host drive exactly the same code path.
//!
//! Input reaches the loop through one plain resource, [`DriveControls`]. The
//! keyboard sampler is the default writer, not a privileged one: a host with its
//! own input plugin (Renzora) or a wheel writes the same three values and the
//! physics path is unchanged — see `docs/RENZORA_MIGRATION.md`.

use bevy::prelude::*;

use super::keymap::key_code_from_vk;
use super::SimulationPause;
use crate::simulator::drivetrain::GearboxModel;
use crate::simulator::input::{InputManager, KeyboardMapping};
use crate::simulator::physics::{integrate_transform, step_vehicle, VehicleConfig, VehicleState};
use crate::simulator::r#loop::{SimulationLoop, PHYSICS_DT};

/// The driving keys, as the shared [`KeyboardMapping`] the menu edits.
///
/// The arrow keys stay fixed alternates ([`KeyboardMapping::ALT_THROTTLE`] and
/// friends), exactly as the C++ `InputManager` treats them.
#[derive(Resource, Clone, Copy, Debug, Default)]
pub struct DriveBindings(pub KeyboardMapping);

/// The vehicle controls for one frame, as plain data.
///
/// This is the seam between *who provides input* and *what the car does*. The
/// drive loop reads only this resource: by default the keyboard sampler is its
/// writer ([`KeyboardDriveInput`]), but a host with its own input plugin
/// (Renzora) — or the ported wheel/gamepad [`InputManager`] — writes the same
/// three values and the physics path does not change. Nothing in the loop reads
/// `ButtonInput<KeyCode>` directly.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct DriveControls {
    /// Accelerator pedal, `0..=1`.
    pub throttle: f32,
    /// Brake pedal, `0..=1`.
    pub brake: f32,
    /// Steering: `-1` full left, `+1` full right.
    pub steer: f32,
}

impl DriveControls {
    /// These controls clamped into their documented ranges.
    ///
    /// Applied on the way into the car, so a host cannot hand the physics an
    /// out-of-range pedal or steering angle.
    pub fn clamped(self) -> Self {
        Self {
            throttle: self.throttle.clamp(0.0, 1.0),
            brake: self.brake.clamp(0.0, 1.0),
            steer: self.steer.clamp(-1.0, 1.0),
        }
    }

    /// The controls the ported [`InputManager`] currently reports.
    ///
    /// The manager already applies the mapping, dead zone and gamma curves, so
    /// this is a conversion rather than a second input path.
    pub fn from_input_manager(manager: &InputManager) -> Self {
        Self {
            throttle: manager.throttle() as f32,
            brake: manager.brake() as f32,
            steer: manager.steer() as f32,
        }
    }
}

/// Whether the built-in keyboard sampler drives [`DriveControls`].
///
/// `true` for the game binary. A host that owns input — Renzora's input plugin,
/// or a wheel backend that writes the controls itself — inserts `false` and
/// never has a key press overwrite what it wrote.
#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyboardDriveInput(pub bool);

impl Default for KeyboardDriveInput {
    fn default() -> Self {
        Self(true)
    }
}

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
            .init_resource::<DriveBindings>()
            .init_resource::<DriveControls>()
            .init_resource::<KeyboardDriveInput>()
            .add_systems(Startup, spawn_car)
            // Menus hold the car: `SimulationPause` stops both the input sample
            // and the fixed-step integration, so no time is banked up to spend
            // in one burst when the menu closes.
            .add_systems(
                Update,
                (
                    read_drive_input.run_if(keyboard_drive_enabled),
                    apply_drive_controls,
                    step_drive,
                )
                    .chain()
                    .run_if(driving_enabled),
            );
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

/// `true` when the bound key, or its fixed arrow alternate, is held.
fn held(keys: &ButtonInput<KeyCode>, vk: i32, alternate: Option<i32>) -> bool {
    let primary = key_code_from_vk(vk).is_some_and(|code| keys.pressed(code));
    let alternate = alternate
        .and_then(key_code_from_vk)
        .is_some_and(|code| keys.pressed(code));
    primary || alternate
}

/// The drive loop runs unless a menu is holding the simulation.
fn driving_enabled(pause: Option<Res<SimulationPause>>) -> bool {
    match pause {
        Some(pause) => !pause.0,
        None => true,
    }
}

/// `true` unless the host took over input.
fn keyboard_drive_enabled(input: Option<Res<KeyboardDriveInput>>) -> bool {
    input.is_none_or(|input| input.0)
}

/// Sample the keyboard into [`DriveControls`] through the rebindable
/// [`DriveBindings`]. Digital keys are full-scale; the steering ramp in
/// `step_vehicle` turns them into a progressive lock.
fn read_drive_input(
    keys: Res<ButtonInput<KeyCode>>,
    bindings: Res<DriveBindings>,
    mut controls: ResMut<DriveControls>,
) {
    let map = bindings.0;
    let throttle = f32::from(u8::from(held(
        &keys,
        map.throttle,
        Some(KeyboardMapping::ALT_THROTTLE),
    )));
    let brake = f32::from(u8::from(held(
        &keys,
        map.brake,
        Some(KeyboardMapping::ALT_BRAKE),
    )));
    let left = f32::from(u8::from(held(
        &keys,
        map.steer_left,
        Some(KeyboardMapping::ALT_STEER_LEFT),
    )));
    let right = f32::from(u8::from(held(
        &keys,
        map.steer_right,
        Some(KeyboardMapping::ALT_STEER_RIGHT),
    )));

    *controls = DriveControls {
        throttle,
        brake,
        steer: right - left,
    };
}

/// Write [`DriveControls`] into every car's [`VehicleState`] inputs.
///
/// The only writer of the drive inputs, whichever side fills the resource: the
/// keyboard sampler above, a host's input plugin, or a wheel. Values are clamped
/// here so the physics never sees an out-of-range pedal.
fn apply_drive_controls(controls: Res<DriveControls>, mut cars: Query<&mut VehicleState>) {
    let controls = controls.clamped();
    for mut car in &mut cars {
        car.throttle = controls.throttle;
        car.brake = controls.brake;
        car.steer_input = controls.steer;
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
    fn driving_is_gated_by_the_pause_flag() {
        let mut world = World::new();
        assert!(
            world.run_system_once(driving_enabled).expect("condition"),
            "no pause resource means the game runs"
        );

        world.insert_resource(SimulationPause(true));
        assert!(
            !world.run_system_once(driving_enabled).expect("condition"),
            "a paused simulation must not step"
        );

        world.resource_mut::<SimulationPause>().0 = false;
        assert!(world.run_system_once(driving_enabled).expect("condition"));
    }

    #[test]
    fn rebound_keys_reach_the_vehicle() {
        let mut world = World::new();
        world.init_resource::<DriveBindings>();
        world.init_resource::<DriveControls>();
        world.resource_mut::<DriveBindings>().0.throttle = b'I' as i32;

        let mut input = ButtonInput::<KeyCode>::default();
        input.press(KeyCode::KeyI);
        world.insert_resource(input);

        let car = world.spawn(VehicleState::default()).id();
        world.run_system_once(read_drive_input).expect("input");
        world.run_system_once(apply_drive_controls).expect("apply");
        assert_eq!(world.get::<VehicleState>(car).expect("car").throttle, 1.0);

        // The default key is no longer read once the binding moved away.
        let mut input = ButtonInput::<KeyCode>::default();
        input.press(KeyCode::KeyW);
        world.insert_resource(input);
        world.run_system_once(read_drive_input).expect("input");
        world.run_system_once(apply_drive_controls).expect("apply");
        assert_eq!(world.get::<VehicleState>(car).expect("car").throttle, 0.0);
    }

    #[test]
    fn a_host_can_drive_without_the_keyboard() {
        let mut world = World::new();
        world.init_resource::<DriveControls>();
        world.init_resource::<KeyboardDriveInput>();
        assert!(
            world.run_system_once(keyboard_drive_enabled).expect("cond"),
            "the keyboard drives by default"
        );

        // The host takes over input and writes the controls itself.
        world.insert_resource(KeyboardDriveInput(false));
        assert!(!world.run_system_once(keyboard_drive_enabled).expect("cond"));
        world.resource_mut::<DriveControls>().throttle = 0.7;
        let mut input = ButtonInput::<KeyCode>::default();
        input.press(KeyCode::KeyS);
        world.insert_resource(input);

        let car = world.spawn(VehicleState::default()).id();

        // The drive loop's own gating expression: with the host in charge the
        // sampler must not run, so a held key cannot reach the controls.
        let mut schedule = bevy::ecs::schedule::Schedule::default();
        schedule.add_systems(read_drive_input.run_if(keyboard_drive_enabled));
        schedule.run(&mut world);
        assert_eq!(world.resource::<DriveControls>().throttle, 0.7);
        assert_eq!(world.resource::<DriveControls>().brake, 0.0);

        world.run_system_once(apply_drive_controls).expect("apply");
        let state = world.get::<VehicleState>(car).expect("car");
        assert_eq!(state.throttle, 0.7);
        assert_eq!(state.brake, 0.0, "a held key must not be read");
    }

    #[test]
    fn out_of_range_controls_are_clamped() {
        let mut world = World::new();
        world.insert_resource(DriveControls {
            throttle: 4.0,
            brake: -2.0,
            steer: -9.0,
        });
        let car = world.spawn(VehicleState::default()).id();
        world.run_system_once(apply_drive_controls).expect("apply");

        let state = world.get::<VehicleState>(car).expect("car");
        assert_eq!(state.throttle, 1.0);
        assert_eq!(state.brake, 0.0);
        assert_eq!(state.steer_input, -1.0);
    }

    #[test]
    fn the_input_manager_feeds_the_same_controls() {
        use crate::simulator::input::{JoystickMapping, AXIS_COUNT, AXIS_LT, AXIS_LX, AXIS_RT};

        let mut manager = InputManager::new();
        manager.set_mapping(JoystickMapping::gamepad_default());
        let mut axes = [0.0; AXIS_COUNT];
        axes[AXIS_RT] = 1.0;
        axes[AXIS_LT] = 0.5;
        axes[AXIS_LX] = 0.5;
        manager.inject_axes(axes, 0);
        manager.update();

        let controls = DriveControls::from_input_manager(&manager);
        assert!(
            (controls.throttle - 1.0).abs() < 1e-6,
            "{} ",
            controls.throttle
        );
        assert!(
            controls.brake > 0.0 && controls.brake < 1.0,
            "half a pedal: {}",
            controls.brake
        );
        assert!(controls.steer > 0.0, "steer {}", controls.steer);
    }

    #[test]
    fn throttle_moves_the_car_forward() {
        let mut world = World::new();
        world.insert_resource(VehicleConfig::default());
        world.insert_resource(SimulationLoop::default());
        world.init_resource::<AutoGearbox>();
        world.init_resource::<DriveBindings>();

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
