//! Sospensioni semplificate — carico verticale, damper, ARB.

use crate::simulator::lut::Lut1D;

#[derive(Clone, Debug)]
pub struct AxleSuspension {
    pub spring_rate: f32,
    pub damper: f32,
    pub ride_height: f32,
    pub bump_lut: Lut1D,
    pub rebound_lut: Lut1D,
}

impl Default for AxleSuspension {
    fn default() -> Self {
        Self {
            spring_rate: 120_000.0,
            damper: 4000.0,
            ride_height: 0.05,
            bump_lut: Lut1D::default(),
            rebound_lut: Lut1D::default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SuspensionConfig {
    pub front: AxleSuspension,
    pub rear: AxleSuspension,
    pub arb_front: Lut1D,
    pub arb_rear: Lut1D,
}

#[derive(Clone, Copy, Debug)]
pub struct VerticalLoads {
    pub fz_front: f32,
    pub fz_rear: f32,
}

/// Carichi verticali statici + aero + trasferimento longitudinale grezzo.
pub fn compute_loads(
    mass: f32,
    cg_location: f32, // fraction toward rear (typical ~0.47)
    downforce: f32,
    long_accel: f32,
    wheelbase: f32,
) -> VerticalLoads {
    let weight = mass * 9.81;
    let cg = cg_location.clamp(0.3, 0.7);
    // static: more load on rear if cg > 0.5
    let mut fz_f = weight * (1.0 - cg) + downforce * 0.45;
    let mut fz_r = weight * cg + downforce * 0.55;
    // longitudinal load transfer
    let transfer = mass * long_accel * (0.4) / wheelbase.max(1.0); // h_cg≈0.4m
    fz_f -= transfer;
    fz_r += transfer;
    VerticalLoads {
        fz_front: fz_f.max(100.0),
        fz_rear: fz_r.max(100.0),
    }
}

pub fn damper_force(axle: &AxleSuspension, compression_vel: f32) -> f32 {
    let base = axle.damper * compression_vel;
    if compression_vel >= 0.0 {
        if axle.bump_lut.is_empty() {
            base
        } else {
            axle.bump_lut.sample(compression_vel.abs()) * compression_vel.signum()
        }
    } else if axle.rebound_lut.is_empty() {
        base
    } else {
        -axle.rebound_lut.sample(compression_vel.abs())
    }
}
