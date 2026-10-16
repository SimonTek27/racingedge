//! Motore ICE — curva di coppia, RPM, limiter.

#[derive(Clone, Debug)]
pub struct EngineConfig {
    pub torque_curve: Vec<(f32, f32)>, // rpm → Nm
    pub rpm_limiter: f32,
    pub rpm_idle: f32,
    pub inertia: f32,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            torque_curve: vec![(2000.0, 400.0), (6000.0, 600.0), (8000.0, 500.0)],
            rpm_limiter: 8000.0,
            rpm_idle: 1200.0,
            inertia: 0.08,
        }
    }
}

#[derive(Clone, Debug)]
pub struct EngineState {
    pub rpm: f32,
    pub torque_nm: f32,
    pub throttle: f32,
}

impl Default for EngineState {
    fn default() -> Self {
        Self {
            rpm: 1200.0,
            torque_nm: 0.0,
            throttle: 0.0,
        }
    }
}

impl EngineConfig {
    pub fn torque_at_rpm(&self, rpm: f32) -> f32 {
        let c = &self.torque_curve;
        if c.is_empty() {
            return 0.0;
        }
        if rpm <= c[0].0 {
            return c[0].1 * (rpm / c[0].0).max(0.0);
        }
        if rpm >= c.last().unwrap().0 {
            return c.last().unwrap().1;
        }
        for i in 0..c.len() - 1 {
            let (r0, t0) = c[i];
            let (r1, t1) = c[i + 1];
            if rpm >= r0 && rpm <= r1 {
                let t = (rpm - r0) / (r1 - r0);
                return t0 + t * (t1 - t0);
            }
        }
        0.0
    }
}

/// Aggiorna RPM da velocità ruota e calcola coppia con throttle + limiter.
pub fn step_engine(
    cfg: &EngineConfig,
    state: &mut EngineState,
    wheel_omega: f32,
    total_gear_ratio: f32,
    throttle: f32,
    dt: f32,
) {
    state.throttle = throttle.clamp(0.0, 1.0);
    if total_gear_ratio.abs() > 0.01 {
        let engine_omega = wheel_omega * total_gear_ratio.abs();
        let target_rpm = engine_omega * 60.0 / std::f32::consts::TAU;
        state.rpm += (target_rpm - state.rpm) * (8.0 * dt).min(1.0);
    } else {
        state.rpm += (cfg.rpm_idle - state.rpm) * 2.0 * dt;
    }
    state.rpm = state.rpm.clamp(cfg.rpm_idle * 0.8, cfg.rpm_limiter + 200.0);

    let cut = if state.rpm >= cfg.rpm_limiter { 0.0 } else { 1.0 };
    state.torque_nm = cfg.torque_at_rpm(state.rpm) * state.throttle * cut;
}
