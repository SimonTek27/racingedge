//! Cinematica delle sospensioni — curve di ammortizzatore, camber, bump-steer e
//! barra antirollio (port di ksengine `SuspensionKinematics.h`).
//!
//! Tutte le curve sono LUT campionate e interpolate linearmente, con clamp agli
//! estremi. L'antirollio genera una forza proporzionale alla differenza di
//! escursione tra i due lati.

/// Un punto della curva di ammortizzatore (velocità, forza).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamperCurvePoint {
    pub velocity: f32,
    pub force: f32,
}

/// Curve cinematiche di una sospensione (port di `SuspensionKinematics`).
#[derive(Clone, Debug)]
pub struct SuspensionKinematics {
    pub damper_curve: Vec<DamperCurvePoint>,
    /// (escursione, camber) — LUT.
    pub camber_curve: Vec<(f32, f32)>,
    /// (escursione, bump-steer) — LUT.
    pub bump_steer_curve: Vec<(f32, f32)>,
    pub anti_roll_bar_stiffness: f32,
    pub anti_roll_bar_enabled: bool,
}

impl Default for SuspensionKinematics {
    fn default() -> Self {
        Self {
            damper_curve: Vec::new(),
            camber_curve: Vec::new(),
            bump_steer_curve: Vec::new(),
            anti_roll_bar_stiffness: 15000.0,
            anti_roll_bar_enabled: true,
        }
    }
}

impl SuspensionKinematics {
    /// Curve stock (port di `stock()`).
    pub fn stock() -> Self {
        Self {
            damper_curve: vec![
                DamperCurvePoint {
                    velocity: -1.0,
                    force: -4500.0,
                },
                DamperCurvePoint {
                    velocity: -0.3,
                    force: -2600.0,
                },
                DamperCurvePoint {
                    velocity: -0.05,
                    force: -400.0,
                },
                DamperCurvePoint {
                    velocity: 0.0,
                    force: 0.0,
                },
                DamperCurvePoint {
                    velocity: 0.05,
                    force: 350.0,
                },
                DamperCurvePoint {
                    velocity: 0.3,
                    force: 1800.0,
                },
                DamperCurvePoint {
                    velocity: 1.0,
                    force: 3200.0,
                },
            ],
            camber_curve: vec![(-0.05, 0.6), (0.0, 0.0), (0.05, -0.9)],
            bump_steer_curve: vec![(-0.05, -0.25), (0.0, 0.0), (0.05, 0.3)],
            ..Default::default()
        }
    }

    /// Forza dell'ammortizzatore a una data velocità (port di `damperForce`).
    pub fn damper_force(&self, v: f32) -> f32 {
        if self.damper_curve.is_empty() {
            return v * 2500.0;
        }
        if v <= self.damper_curve[0].velocity {
            return self.damper_curve[0].force;
        }
        let last = self.damper_curve.len() - 1;
        if v >= self.damper_curve[last].velocity {
            return self.damper_curve[last].force;
        }
        for i in 0..self.damper_curve.len() - 1 {
            let a = self.damper_curve[i];
            let b = self.damper_curve[i + 1];
            if v >= a.velocity && v <= b.velocity {
                let t = (v - a.velocity) / (b.velocity - a.velocity).max(1e-6);
                return a.force + t * (b.force - a.force);
            }
        }
        v * 2500.0
    }

    /// Camber a una data escursione.
    pub fn camber_from_travel(&self, travel: f32) -> f32 {
        lut(&self.camber_curve, travel)
    }

    /// Bump-steer a una data escursione.
    pub fn bump_steer_from_travel(&self, travel: f32) -> f32 {
        lut(&self.bump_steer_curve, travel)
    }

    /// Forza di antirollio da differenza di escursione (port di `antiRollForce`).
    pub fn anti_roll_force(&self, left_travel: f32, right_travel: f32) -> f32 {
        if !self.anti_roll_bar_enabled {
            return 0.0;
        }
        (right_travel - left_travel) * self.anti_roll_bar_stiffness * 0.5
    }
}

fn lut(curve: &[(f32, f32)], x: f32) -> f32 {
    if curve.is_empty() {
        return 0.0;
    }
    if x <= curve[0].0 {
        return curve[0].1;
    }
    let last = curve.len() - 1;
    if x >= curve[last].0 {
        return curve[last].1;
    }
    for i in 0..curve.len() - 1 {
        let (x0, y0) = curve[i];
        let (x1, y1) = curve[i + 1];
        if x >= x0 && x <= x1 {
            let t = (x - x0) / (x1 - x0).max(1e-6);
            return y0 + t * (y1 - y0);
        }
    }
    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stock_damper_hits_curve_points() {
        let k = SuspensionKinematics::stock();
        assert_eq!(k.damper_force(0.0), 0.0);
        assert_eq!(k.damper_force(0.3), 1800.0);
        assert_eq!(k.damper_force(-0.3), -2600.0);
    }

    #[test]
    fn damper_clamps_at_extremes() {
        let k = SuspensionKinematics::stock();
        assert_eq!(k.damper_force(-5.0), -4500.0);
        assert_eq!(k.damper_force(5.0), 3200.0);
    }

    #[test]
    fn damper_interpolates_between_points() {
        let k = SuspensionKinematics::stock();
        // Between (0.0, 0) and (0.05, 350): halfway -> 175.
        assert!((k.damper_force(0.025) - 175.0).abs() < 1e-3);
    }

    #[test]
    fn empty_damper_uses_linear_fallback() {
        let k = SuspensionKinematics::default();
        assert_eq!(k.damper_force(2.0), 5000.0);
    }

    #[test]
    fn camber_and_bump_steer_luts() {
        let k = SuspensionKinematics::stock();
        assert_eq!(k.camber_from_travel(0.0), 0.0);
        assert_eq!(k.camber_from_travel(-0.05), 0.6);
        assert_eq!(k.camber_from_travel(0.05), -0.9);
        assert_eq!(k.camber_from_travel(-1.0), 0.6, "clamped low");
        assert_eq!(k.camber_from_travel(1.0), -0.9, "clamped high");
        assert!((k.camber_from_travel(-0.025) - 0.3).abs() < 1e-3);

        assert_eq!(k.bump_steer_from_travel(0.0), 0.0);
        assert_eq!(k.bump_steer_from_travel(0.05), 0.3);
    }

    #[test]
    fn anti_roll_force_depends_on_difference() {
        let k = SuspensionKinematics::stock();
        // right lower than left -> negative (pushes the other way).
        assert_eq!(k.anti_roll_force(0.1, 0.0), -0.1 * 15000.0 * 0.5);
        assert_eq!(k.anti_roll_force(0.0, 0.1), 750.0);
        assert_eq!(k.anti_roll_force(0.05, 0.05), 0.0);
    }

    #[test]
    fn disabled_anti_roll_is_zero() {
        let mut k = SuspensionKinematics::stock();
        k.anti_roll_bar_enabled = false;
        assert_eq!(k.anti_roll_force(0.0, 0.5), 0.0);
    }

    #[test]
    fn empty_luts_return_zero() {
        let k = SuspensionKinematics::default();
        assert_eq!(k.camber_from_travel(0.1), 0.0);
        assert_eq!(k.bump_steer_from_travel(0.1), 0.0);
    }
}
