//! Cambio, final drive, rapporto totale, slip ratio.

#[derive(Clone, Debug)]
pub struct DrivetrainConfig {
    pub gear_ratios: Vec<f32>,
    pub final_drive: f32,
    pub efficiency: f32,
}

impl Default for DrivetrainConfig {
    fn default() -> Self {
        Self {
            gear_ratios: vec![2.6, 2.0, 1.7, 1.4, 1.2, 1.1, 1.0],
            final_drive: 3.2,
            efficiency: 0.92,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DrivetrainState {
    /// -1 = R, 0 = N, 1..N = marce
    pub gear: i8,
}

impl Default for DrivetrainState {
    fn default() -> Self {
        Self { gear: 1 }
    }
}

impl DrivetrainConfig {
    pub fn gear_ratio(&self, gear: i8) -> f32 {
        match gear {
            -1 => -self.gear_ratios.first().copied().unwrap_or(2.5) * 0.85,
            1..=8 => self
                .gear_ratios
                .get((gear - 1) as usize)
                .copied()
                .unwrap_or(1.0),
            _ => 0.0,
        }
    }

    pub fn total_ratio(&self, gear: i8) -> f32 {
        self.gear_ratio(gear) * self.final_drive
    }

    /// Forza longitudinale richiesta alle ruote da coppia motore [N].
    pub fn drive_force(&self, engine_torque_nm: f32, gear: i8, wheel_radius: f32) -> f32 {
        let total = self.total_ratio(gear);
        let wheel_tq = engine_torque_nm * total.abs() * self.efficiency * total.signum();
        wheel_tq / wheel_radius.max(0.05)
    }
}

/// Slip ratio κ = (ω·r − vx) / max(|vx|, ε)
#[inline]
pub fn slip_ratio(vx: f32, wheel_omega: f32, radius: f32) -> f32 {
    let vx_abs = vx.abs().max(0.5);
    let vs = wheel_omega * radius - vx;
    (vs / vx_abs).clamp(-1.5, 1.5)
}

#[inline]
pub fn wheel_omega_from_engine(rpm: f32, total_ratio: f32) -> f32 {
    if total_ratio.abs() < 0.01 {
        return 0.0;
    }
    (rpm * std::f32::consts::TAU / 60.0) / total_ratio.abs()
}
