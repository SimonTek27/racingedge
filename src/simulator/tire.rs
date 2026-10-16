//! Magic Formula tire model — port of ksengine KsTireModel (GPL-3.0)
//! Original: https://github.com/SimonTek27/ksengine
//!
//! Core: F = D * sin(C * atan(B*x - E*(B*x - atan(B*x))))
//! Combined slip via friction-circle normalization on forces.

/// Pacejka / Magic Formula coefficients (A lateral, B longitudinal).
#[derive(Clone, Debug)]
pub struct TireCoefficients {
    // Lateral (A)
    pub a1: f32,
    pub a2: f32,
    pub a3: f32,
    pub a4: f32,
    pub a5: f32,
    pub a6: f32,
    pub a7: f32,
    // Longitudinal (B)
    pub b1: f32,
    pub b2: f32,
    pub b3: f32,
    pub b4: f32,
    pub b5: f32,
    pub b6: f32,
    pub b7: f32,
    // Load / env
    pub nominal_load_n: f32,
    pub load_sens_exp: f32,
    pub load_sens_min: f32,
    pub load_sens_max: f32,
    pub opt_pressure_psi: f32,
    pub opt_temp_c: f32,
}

impl Default for TireCoefficients {
    fn default() -> Self {
        Self::slick()
    }
}

impl TireCoefficients {
    /// Race slick (default in ksengine)
    pub fn slick() -> Self {
        Self {
            a1: -22.1,
            a2: 1100.0,
            a3: 1200.0,
            a4: 1.82,
            a5: 0.208,
            a6: 0.0,
            a7: -0.354,
            b1: -21.3,
            b2: 1200.0,
            b3: 49.6,
            b4: 226.0,
            b5: 0.069,
            b6: -0.006,
            b7: 0.056,
            nominal_load_n: 4000.0,
            load_sens_exp: 0.9,
            load_sens_min: 0.55,
            load_sens_max: 1.25,
            opt_pressure_psi: 26.0,
            opt_temp_c: 80.0,
        }
    }

    pub fn street() -> Self {
        let mut c = Self::slick();
        c.a2 = 900.0;
        c.b2 = 1000.0;
        c.a3 = 1078.0;
        c
    }

    pub fn wet() -> Self {
        let mut c = Self::slick();
        c.a2 = 700.0;
        c.b2 = 750.0;
        c
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TireState {
    /// Slip angle [rad]
    pub slip_angle: f32,
    /// Slip ratio [-]  (omega*r - vx) / |vx|
    pub slip_ratio: f32,
    /// Vertical load [N]
    pub normal_force: f32,
    /// Camber [rad]
    pub camber: f32,
    /// Surface friction multiplier
    pub mu: f32,
    /// Tire pressure [psi]
    pub pressure_psi: f32,
    /// Tire temperature [°C]
    pub temp_c: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TireForces {
    pub fx: f32, // longitudinal [N]
    pub fy: f32, // lateral [N]
    pub mz: f32, // aligning moment [N·m]
}

#[derive(Clone)]
struct CoeffCache {
    valid: bool,
    fz: f32,
    camber_abs: f32,
    mu: f32,
    dy: f32,
    by: f32,
    cy: f32,
    ey: f32,
    dx: f32,
    bx: f32,
    cx: f32,
    ex: f32,
}

impl Default for CoeffCache {
    fn default() -> Self {
        Self {
            valid: false,
            fz: -1.0,
            camber_abs: -1.0,
            mu: -1.0,
            dy: 0.0,
            by: 10.0,
            cy: 1.3,
            ey: 0.0,
            dx: 0.0,
            bx: 10.0,
            cx: 1.65,
            ex: 0.0,
        }
    }
}

/// Magic Formula tire model (port of ksengine KsTireModel).
#[derive(Clone)]
pub struct MagicFormulaTire {
    coeffs: TireCoefficients,
    cache: CoeffCache,
}

impl Default for MagicFormulaTire {
    fn default() -> Self {
        Self::new(TireCoefficients::slick())
    }
}

impl MagicFormulaTire {
    pub fn new(coeffs: TireCoefficients) -> Self {
        Self {
            coeffs,
            cache: CoeffCache::default(),
        }
    }

    pub fn slick() -> Self {
        Self::new(TireCoefficients::slick())
    }

    /// Classic Magic Formula:
    /// F = D * sin( C * atan( B*x - E*(B*x - atan(B*x)) ) )
    #[inline]
    pub fn magic_formula(x: f32, b: f32, c: f32, d: f32, e: f32) -> f32 {
        let bx = b * x;
        let inner = bx - e * (bx - bx.atan());
        d * (c * inner.atan()).sin()
    }

    fn load_sensitivity(&self, fz: f32) -> f32 {
        let fz = fz.max(1.0);
        let fz0 = self.coeffs.nominal_load_n.max(500.0);
        let ratio = fz / fz0;
        let expn = self.coeffs.load_sens_exp.clamp(0.5, 1.2);
        let s = if (ratio - 1.0).abs() < 0.25 {
            let d = ratio - 1.0;
            1.0 + expn * d + 0.5 * expn * (expn - 1.0) * d * d
        } else {
            ratio.powf(expn)
        };
        s.clamp(self.coeffs.load_sens_min, self.coeffs.load_sens_max)
    }

    fn temperature_effect(&self, temp: f32) -> f32 {
        let opt = if self.coeffs.opt_temp_c > 1.0 {
            self.coeffs.opt_temp_c
        } else {
            80.0
        };
        let d = (temp - opt) / 30.0;
        (1.0 - d * d * 0.25).clamp(0.65, 1.05)
    }

    fn pressure_effect(&self, pressure_psi: f32) -> f32 {
        let opt = if self.coeffs.opt_pressure_psi > 1.0 {
            self.coeffs.opt_pressure_psi
        } else {
            26.0
        };
        let d = (pressure_psi - opt) / 8.0;
        (1.0 - d * d * 0.15).clamp(0.7, 1.05)
    }

    fn ensure_cache(&mut self, fz: f32, camber_abs: f32, mu: f32) {
        const FZ_TOL: f32 = 25.0;
        const CAM_TOL: f32 = 0.01;
        const MU_TOL: f32 = 0.02;
        if self.cache.valid
            && (fz - self.cache.fz).abs() < FZ_TOL
            && (camber_abs - self.cache.camber_abs).abs() < CAM_TOL
            && (mu - self.cache.mu).abs() < MU_TOL
        {
            return;
        }

        let a = &self.coeffs;
        let fz_kn = fz * 0.001;
        let load_sens = self.load_sensitivity(fz);

        // Lateral peak Dy and stiffness
        let mut dy = (a.a1 * fz_kn + a.a2) * fz_kn;
        dy = dy.abs() * mu;
        let z = fz_kn / if a.a4 > 1e-4 { a.a4 } else { 1.82 };
        let sin2atan = 2.0 * z / (1.0 + z * z);
        let bcd_y = a.a3 * sin2atan * (1.0 - a.a5 * camber_abs);
        let by = (bcd_y.abs() / (dy + 1e-3)).clamp(0.1, 40.0);
        let cy = 1.3;
        let ey = a.a6 * fz_kn + a.a7;

        // Longitudinal
        let mut dx = (a.b1 * fz_kn + a.b2) * fz_kn;
        dx = dx.abs() * mu * load_sens;
        let dx_floor = 1.05 * mu * load_sens * fz;
        if dx < dx_floor {
            dx = dx_floor;
        }
        let exp_arg = -a.b5 * fz_kn;
        let exp_term = if exp_arg < -1.0 {
            exp_arg.exp()
        } else {
            1.0 + exp_arg + 0.5 * exp_arg * exp_arg
        };
        let mut bcd_x = (a.b3 * fz_kn * fz_kn + a.b4 * fz_kn) * exp_term;
        if bcd_x.abs() < 10.0 * dx {
            bcd_x = 10.0 * dx;
        }
        let bx = (bcd_x.abs() / (dx + 1e-3)).clamp(0.1, 40.0);
        let cx = 1.65;
        let ex = a.b6 * fz_kn + a.b7;

        self.cache = CoeffCache {
            valid: true,
            fz,
            camber_abs,
            mu,
            dy,
            by,
            cy,
            ey,
            dx,
            bx,
            cx,
            ex,
        };
    }

    /// Compute tire forces for given state (combined slip).
    pub fn calculate_forces(&mut self, state: &TireState) -> TireForces {
        let fz = state.normal_force.max(1.0);
        let mu = state.mu.max(0.1);
        let cam_abs = state.camber.abs();

        self.ensure_cache(fz, cam_abs, mu);

        let alpha = state.slip_angle;
        let kappa = state.slip_ratio;

        let mut fy = Self::magic_formula(
            alpha,
            self.cache.by,
            self.cache.cy,
            self.cache.dy,
            self.cache.ey,
        );
        // small camber thrust
        fy += fz * state.camber * 0.1;

        let mut fx = Self::magic_formula(
            kappa,
            self.cache.bx,
            self.cache.cx,
            self.cache.dx,
            self.cache.ex,
        );

        // Combined slip: friction-circle on forces
        let nx = fx / (self.cache.dx + 1e-3);
        let ny = fy / (self.cache.dy + 1e-3);
        let comb = (nx * nx + ny * ny).sqrt();
        if comb > 1.0 {
            fx /= comb;
            fy /= comb;
        }

        let env = self.temperature_effect(state.temp_c) * self.pressure_effect(state.pressure_psi);
        fx *= env;
        fy *= env;

        TireForces {
            fx,
            fy,
            mz: -fy * 0.03,
        }
    }

    /// Peak lateral grip magnitude at given load.
    pub fn peak_lateral(&mut self, fz: f32) -> f32 {
        let s = TireState {
            slip_angle: 8.0_f32.to_radians(),
            normal_force: fz,
            mu: 1.0,
            pressure_psi: 26.0,
            temp_c: 80.0,
            ..Default::default()
        };
        self.calculate_forces(&s).fy.abs()
    }

    /// Peak longitudinal grip magnitude at given load.
    pub fn peak_longitudinal(&mut self, fz: f32) -> f32 {
        let s = TireState {
            slip_ratio: 0.12,
            normal_force: fz,
            mu: 1.0,
            pressure_psi: 26.0,
            temp_c: 80.0,
            ..Default::default()
        };
        self.calculate_forces(&s).fx.abs()
    }
}

/// Approximate slip angle for bicycle model [rad].
/// alpha ≈ atan(vy / |vx|) - delta   (front) or atan(vy / |vx|) (rear)
#[inline]
pub fn slip_angle_front(vx: f32, vy: f32, yaw_rate: f32, steer: f32, wheelbase: f32) -> f32 {
    let vx = vx.max(0.5); // avoid singularity at rest
    let a = wheelbase * 0.5; // distance CG → front (simplified equal)
    (vy + a * yaw_rate).atan2(vx) - steer
}

#[inline]
pub fn slip_angle_rear(vx: f32, vy: f32, yaw_rate: f32, wheelbase: f32) -> f32 {
    let vx = vx.max(0.5);
    let b = wheelbase * 0.5;
    (vy - b * yaw_rate).atan2(vx)
}

/// Slip ratio from longitudinal speed and driven wheel speed.
/// kappa = (ω r - vx) / max(|vx|, ε)
#[inline]
pub fn slip_ratio(vx: f32, wheel_omega: f32, radius: f32) -> f32 {
    let vx_abs = vx.abs().max(0.5);
    let vs = wheel_omega * radius - vx;
    (vs / vx_abs).clamp(-1.5, 1.5)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_formula_zero() {
        let f = MagicFormulaTire::magic_formula(0.0, 10.0, 1.3, 4000.0, 0.0);
        assert!(f.abs() < 1.0);
    }

    #[test]
    fn peak_positive() {
        let mut t = MagicFormulaTire::slick();
        let fy = t.peak_lateral(4000.0);
        let fx = t.peak_longitudinal(4000.0);
        // `peak_*` campiona a 8° / 0.12 come `calculatePeak*Grip` di ksengine,
        // non il massimo matematico della curva. Su questa slick `By ≈ 0.2`
        // (ksengine tiene la curva laterale ancora in salita), quindi il
        // laterale resta molto sotto `mu * Fz`: qui conta solo il segno.
        assert!(fy > 0.0, "fy={fy}");
        assert!(fx > 0.0, "fx={fx}");
    }

    #[test]
    fn longitudinal_grip_tracks_load() {
        let mut t = MagicFormulaTire::slick();
        // Il picco longitudinale è pavimentato a ~1.05 * mu * Fz.
        let light = t.peak_longitudinal(2000.0);
        let heavy = t.peak_longitudinal(6000.0);
        assert!(light > 0.4 * 2000.0, "light={light}");
        assert!(heavy > light, "light={light} heavy={heavy}");
    }
}
