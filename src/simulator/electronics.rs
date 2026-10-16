//! Elettronica / hybrid — ERS, deploy regolamentare LMH.

use crate::simulator::lut::Lut1D;

#[derive(Clone, Debug)]
pub struct ErsConfig {
    pub enabled: bool,
    pub torque_lut: Lut1D,       // rpm → Nm
    pub front_torque_lut: Lut1D, // rpm → Nm asse anteriore
    pub coast_torque_lut: Lut1D,
    pub max_front_kw: f32,
    pub deploy_speed_kmh: f32,
}

impl Default for ErsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            torque_lut: Lut1D::default(),
            front_torque_lut: Lut1D::default(),
            coast_torque_lut: Lut1D::default(),
            max_front_kw: 200.0,
            deploy_speed_kmh: 190.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ErsOutput {
    /// Coppia elettrica anteriore [Nm]
    pub front_torque_nm: f32,
    /// Coppia elettrica posteriore / MGU-K [Nm]
    pub rear_torque_nm: f32,
}

/// Coppia ERS in base a RPM, velocità e throttle (gate regolamentare).
pub fn compute_ers(cfg: &ErsConfig, rpm: f32, speed_kmh: f32, throttle: f32) -> ErsOutput {
    if !cfg.enabled || throttle < 0.05 {
        return ErsOutput::default();
    }
    let mut out = ErsOutput::default();

    // Front axle (LMH: typically above deploy speed)
    if speed_kmh >= cfg.deploy_speed_kmh {
        out.front_torque_nm = if cfg.front_torque_lut.is_empty() {
            let omega = (rpm * 0.1).max(50.0) * std::f32::consts::TAU / 60.0;
            (cfg.max_front_kw * 1000.0 / omega).min(800.0) * throttle
        } else {
            cfg.front_torque_lut.sample(rpm) * throttle
        };
    }

    if !cfg.torque_lut.is_empty() {
        out.rear_torque_nm = cfg.torque_lut.sample(rpm) * throttle;
    }

    out
}
