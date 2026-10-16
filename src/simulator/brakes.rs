//! Freni — coppia max e ripartizione anteriore/posteriore.

#[derive(Clone, Debug)]
pub struct BrakeConfig {
    pub max_torque_nm: f32,
    pub front_share: f32,
}

impl Default for BrakeConfig {
    fn default() -> Self {
        Self {
            max_torque_nm: 4000.0,
            front_share: 0.6,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BrakeForces {
    pub fx_front: f32,
    pub fx_rear: f32,
}

/// Calcola forze frenanti longitudinali [N] limitate dal picco pneumatico.
pub fn compute_brakes(
    cfg: &BrakeConfig,
    brake_input: f32,
    speed: f32,
    wheel_radius: f32,
    peak_fx_front: f32,
    peak_fx_rear: f32,
) -> BrakeForces {
    let brake = brake_input.clamp(0.0, 1.0);
    if brake < 0.01 || speed.abs() < 0.05 {
        return BrakeForces::default();
    }
    let dir = -speed.signum();
    let total_force = brake * cfg.max_torque_nm / wheel_radius.max(0.05);
    let fx_f = (dir * total_force * cfg.front_share).clamp(-peak_fx_front, peak_fx_front);
    let fx_r = (dir * total_force * (1.0 - cfg.front_share)).clamp(-peak_fx_rear, peak_fx_rear);
    BrakeForces {
        fx_front: fx_f,
        fx_rear: fx_r,
    }
}
