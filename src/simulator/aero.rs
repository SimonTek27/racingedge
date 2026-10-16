//! Aerodinamica — drag e downforce da coefficienti o LUT BodyCD/CL.

use crate::simulator::lut::Lut1D;

#[derive(Clone, Debug)]
pub struct AeroConfig {
    pub cd: f32,
    pub cl: f32,
    pub area: f32,
    pub air_density: f32,
    pub body_cd: Lut1D,
    pub body_cl: Lut1D,
}

impl Default for AeroConfig {
    fn default() -> Self {
        Self {
            cd: 0.55,
            cl: 3.0,
            area: 2.0,
            air_density: 1.225,
            body_cd: Lut1D::default(),
            body_cl: Lut1D::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct AeroForces {
    pub drag: f32,       // N, opposes velocity
    pub downforce: f32,  // N, positive = down
    pub cd: f32,
    pub cl: f32,
}

impl AeroConfig {
    pub fn coeffs_at_pitch(&self, pitch_rad: f32) -> (f32, f32) {
        let deg = pitch_rad.to_degrees();
        let cd = if self.body_cd.is_empty() {
            self.cd
        } else {
            self.body_cd.sample(deg).abs()
        };
        let cl = if self.body_cl.is_empty() {
            self.cl
        } else {
            self.body_cl.sample(deg).abs()
        };
        (cd.clamp(0.15, 2.0), cl.clamp(0.0, 10.0))
    }
}

pub fn compute_aero(cfg: &AeroConfig, speed: f32, pitch_rad: f32) -> AeroForces {
    let (cd, cl) = cfg.coeffs_at_pitch(pitch_rad);
    let v2 = speed * speed;
    let q = 0.5 * cfg.air_density * cfg.area * v2;
    AeroForces {
        drag: q * cd * speed.signum(),
        downforce: q * cl,
        cd,
        cl,
    }
}
