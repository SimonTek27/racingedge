//! Orchestrazione fisica veicolo — chiama engine, drivetrain, brakes, aero,
//! electronics, suspension, tire.

use crate::simulator::aero::{self, AeroConfig, AeroForces};
use crate::simulator::brakes::{self, BrakeConfig};
use crate::simulator::drivetrain::{self, DrivetrainConfig, DrivetrainState};
use crate::simulator::electronics::{self, ErsConfig};
use crate::simulator::engine::{self, EngineConfig, EngineState};
use crate::simulator::suspension::{self, SuspensionConfig};
use crate::simulator::tire::{slip_angle_front, slip_angle_rear, MagicFormulaTire, TireState};
use bevy::prelude::*;

/// Config immutabile derivata dal data pack.
#[derive(Resource, Clone)]
pub struct VehicleConfig {
    pub name: String,
    pub mass: f32,
    pub wheelbase: f32,
    pub cg_location: f32,
    pub wheel_radius: f32,
    pub steer_lock_deg: f32,
    pub steer_ratio: f32,
    pub engine: EngineConfig,
    pub drivetrain: DrivetrainConfig,
    pub brakes: BrakeConfig,
    pub aero: AeroConfig,
    pub ers: ErsConfig,
    pub suspension: SuspensionConfig,
    /// DX/DY ref for pack-style tire forces
    pub dx_ref_f: f32,
    pub dy_ref_f: f32,
    pub dx_ref_r: f32,
    pub dy_ref_r: f32,
    pub fz0_f: f32,
    pub fz0_r: f32,
    pub ls_exp_x_f: f32,
    pub ls_exp_y_f: f32,
    pub ls_exp_x_r: f32,
    pub ls_exp_y_r: f32,
    pub pressure_f: f32,
    pub pressure_r: f32,
    pub camber_f: f32,
    pub camber_r: f32,
    pub rolling_res: f32,
    pub yaw_inertia: f32,
}

impl Default for VehicleConfig {
    fn default() -> Self {
        Self {
            name: "Car".into(),
            mass: 1100.0,
            wheelbase: 3.0,
            cg_location: 0.5,
            wheel_radius: 0.355,
            steer_lock_deg: 270.0,
            steer_ratio: 13.0,
            engine: EngineConfig::default(),
            drivetrain: DrivetrainConfig::default(),
            brakes: BrakeConfig::default(),
            aero: AeroConfig::default(),
            ers: ErsConfig::default(),
            suspension: SuspensionConfig::default(),
            dx_ref_f: 1.4,
            dy_ref_f: 1.55,
            dx_ref_r: 1.5,
            dy_ref_r: 1.6,
            fz0_f: 3000.0,
            fz0_r: 3000.0,
            ls_exp_x_f: 0.86,
            ls_exp_y_f: 0.72,
            ls_exp_x_r: 0.86,
            ls_exp_y_r: 0.72,
            pressure_f: 22.0,
            pressure_r: 22.0,
            camber_f: -2.5,
            camber_r: -2.0,
            rolling_res: 6.5,
            yaw_inertia: 1800.0,
        }
    }
}

/// Stato dinamico del veicolo (component Bevy).
#[derive(Component)]
pub struct VehicleState {
    pub speed: f32,
    pub lat_vel: f32,
    pub yaw: f32,
    pub yaw_rate: f32,
    pub steer_input: f32,
    pub steer_angle: f32,
    pub throttle: f32,
    pub brake: f32,
    pub engine: EngineState,
    pub drivetrain: DrivetrainState,
    pub tire_front: MagicFormulaTire,
    pub tire_rear: MagicFormulaTire,
    pub fx_total: f32,
    pub fy_total: f32,
    pub aero: AeroForces,
}

impl Default for VehicleState {
    fn default() -> Self {
        Self {
            speed: 0.0,
            lat_vel: 0.0,
            yaw: 0.0,
            yaw_rate: 0.0,
            steer_input: 0.0,
            steer_angle: 0.0,
            throttle: 0.0,
            brake: 0.0,
            engine: EngineState::default(),
            drivetrain: DrivetrainState::default(),
            tire_front: MagicFormulaTire::slick(),
            tire_rear: MagicFormulaTire::slick(),
            fx_total: 0.0,
            fy_total: 0.0,
            aero: AeroForces::default(),
        }
    }
}

/// Un passo di simulazione (piano: long + lat + yaw).
pub fn step_vehicle(cfg: &VehicleConfig, car: &mut VehicleState, dt: f32) {
    let dt = dt.min(0.05);
    let wb = cfg.wheelbase;
    let r = cfg.wheel_radius;

    // Sterzo
    let speed_kmh = car.speed.abs() * 3.6;
    let speed_factor = (1.0 - (speed_kmh / 320.0).min(0.85)).max(0.15);
    let max_steer = (cfg.steer_lock_deg / cfg.steer_ratio).to_radians() * 0.5;
    let target = car.steer_input * max_steer * speed_factor;
    car.steer_angle += (target - car.steer_angle) * (12.0 * dt).min(1.0);

    // Drivetrain + engine
    let total_ratio = cfg.drivetrain.total_ratio(car.drivetrain.gear);
    let wheel_omega = if total_ratio.abs() > 0.01 {
        drivetrain::wheel_omega_from_engine(car.engine.rpm, total_ratio)
    } else {
        car.speed / r
    };

    engine::step_engine(
        &cfg.engine,
        &mut car.engine,
        wheel_omega,
        total_ratio,
        car.throttle,
        dt,
    );

    let drive_req = cfg
        .drivetrain
        .drive_force(car.engine.torque_nm, car.drivetrain.gear, r);

    // Aero
    car.aero = aero::compute_aero(&cfg.aero, car.speed, 0.0);

    // Loads
    let loads = suspension::compute_loads(
        cfg.mass,
        cfg.cg_location,
        car.aero.downforce,
        0.0, // refined after forces
        wb,
    );
    let fz_f = loads.fz_front;
    let fz_r = loads.fz_rear;

    // Slip
    let vx = car.speed.max(0.3);
    let alpha_f = slip_angle_front(vx, car.lat_vel, car.yaw_rate, car.steer_angle, wb);
    let alpha_r = slip_angle_rear(vx, car.lat_vel, car.yaw_rate, wb);
    let kappa_r = drivetrain::slip_ratio(car.speed, wheel_omega, r);
    let kappa_f = if car.brake > 0.05 {
        -car.brake * 0.15 * car.speed.signum()
    } else {
        0.0
    };

    // Magic Formula
    let st_f = TireState {
        slip_angle: alpha_f,
        slip_ratio: kappa_f,
        normal_force: fz_f,
        camber: cfg.camber_f.to_radians(),
        mu: 1.05,
        pressure_psi: cfg.pressure_f,
        temp_c: 85.0,
    };
    let st_r = TireState {
        slip_angle: alpha_r,
        slip_ratio: kappa_r,
        normal_force: fz_r,
        camber: cfg.camber_r.to_radians(),
        mu: 1.05,
        pressure_psi: cfg.pressure_r,
        temp_c: 90.0,
    };
    let mf_f = car.tire_front.calculate_forces(&st_f);
    let mf_r = car.tire_rear.calculate_forces(&st_r);

    // Pack-style REF forces
    let pack_fx_f = pack_fx(cfg.dx_ref_f, fz_f, cfg.fz0_f, cfg.ls_exp_x_f, kappa_f);
    let pack_fx_r = pack_fx(cfg.dx_ref_r, fz_r, cfg.fz0_r, cfg.ls_exp_x_r, kappa_r);
    let pack_fy_f = pack_fy(cfg.dy_ref_f, fz_f, cfg.fz0_f, cfg.ls_exp_y_f, alpha_f, 0.12);
    let pack_fy_r = pack_fy(cfg.dy_ref_r, fz_r, cfg.fz0_r, cfg.ls_exp_y_r, alpha_r, 0.12);

    let mut fx_f = pack_fx_f * 0.6 + mf_f.fx * 0.4;
    let mut fx_r = pack_fx_r * 0.6 + mf_r.fx * 0.4;
    let fy_f = pack_fy_f * 0.6 + mf_f.fy * 0.4;
    let fy_r = pack_fy_r * 0.6 + mf_r.fy * 0.4;

    let peak_f = (cfg.dx_ref_f * fz_f).max(500.0);
    let peak_r = (cfg.dx_ref_r * fz_r).max(500.0);

    // ERS
    let ers = electronics::compute_ers(&cfg.ers, car.engine.rpm, speed_kmh, car.throttle);
    let ers_fx = ers.front_torque_nm / r;

    if car.throttle > 0.05 && car.brake < 0.05 {
        let drive_rear = (drive_req * 0.75).clamp(-peak_r, peak_r);
        let drive_front = (drive_req * 0.10 + ers_fx).clamp(-peak_f, peak_f);
        fx_r = fx_r * 0.35 + drive_rear * 0.65;
        fx_f = fx_f * 0.35 + drive_front * 0.65;
    }

    if car.brake > 0.05 {
        let b = brakes::compute_brakes(&cfg.brakes, car.brake, car.speed, r, peak_f, peak_r);
        fx_f = b.fx_front;
        fx_r = b.fx_rear;
    }

    car.fx_total = fx_f + fx_r;
    car.fy_total = fy_f + fy_r;

    // EOM
    let rr = cfg.rolling_res * 0.15 * car.speed.signum();
    let fx_net =
        fx_f * car.steer_angle.cos() + fx_r - car.aero.drag - rr - fy_f * car.steer_angle.sin();
    let ax = fx_net / cfg.mass;
    car.speed += ax * dt;
    if car.speed.abs() < 0.25 && car.brake > 0.5 {
        car.speed = 0.0;
        car.lat_vel = 0.0;
    }

    let fy_net = fy_f * car.steer_angle.cos() + fy_r + fx_f * car.steer_angle.sin();
    let ay = fy_net / cfg.mass - car.speed * car.yaw_rate;
    car.lat_vel += ay * dt;
    if car.speed.abs() < 1.0 {
        car.lat_vel *= 0.9;
    }
    car.lat_vel = car.lat_vel.clamp(-15.0, 15.0);

    let half = wb * 0.5;
    let mz = half * (fy_f * car.steer_angle.cos() + fx_f * car.steer_angle.sin()) - half * fy_r;
    car.yaw_rate += (mz / cfg.yaw_inertia) * dt;
    car.yaw_rate *= 0.998;
    car.yaw_rate = car.yaw_rate.clamp(-2.5, 2.5);
    car.yaw += car.yaw_rate * dt;
}

fn pack_fx(dx_ref: f32, fz: f32, fz0: f32, ls_exp: f32, kappa: f32) -> f32 {
    let sens = (fz / fz0.max(1.0))
        .max(0.1)
        .powf(ls_exp.clamp(0.4, 1.2))
        .clamp(0.5, 1.3);
    let shape = kappa.signum() * (kappa.abs() / 0.12).min(1.0);
    dx_ref * fz * sens * shape
}

fn pack_fy(dy_ref: f32, fz: f32, fz0: f32, ls_exp: f32, alpha: f32, peak: f32) -> f32 {
    let sens = (fz / fz0.max(1.0))
        .max(0.1)
        .powf(ls_exp.clamp(0.4, 1.2))
        .clamp(0.5, 1.3);
    let shape = alpha.signum() * (alpha.abs() / peak.max(0.05)).min(1.0);
    dy_ref * fz * sens * shape
}

/// Integra posizione/rotazione nel Transform Bevy.
pub fn integrate_transform(car: &VehicleState, tf: &mut Transform, dt: f32) {
    let forward = Vec3::new(-car.yaw.sin(), 0.0, -car.yaw.cos());
    let right = Vec3::new(car.yaw.cos(), 0.0, -car.yaw.sin());
    tf.translation += (forward * car.speed + right * car.lat_vel) * dt;
    tf.translation.y = 0.55;
    tf.rotation = Quat::from_rotation_y(car.yaw);
}
