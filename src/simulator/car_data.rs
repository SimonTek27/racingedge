//! Loader completo data pack AC (data.json + .lut) — file originali non modificati.

use crate::simulator::ac_import::AcImporter;
use crate::simulator::aero::AeroConfig;
use crate::simulator::brakes::BrakeConfig;
use crate::simulator::drivetrain::DrivetrainConfig;
use crate::simulator::electronics::ErsConfig;
use crate::simulator::engine::EngineConfig;
use crate::simulator::lut::{synthesize_slip_curve, Lut1D};
use crate::simulator::physics::VehicleConfig;
use crate::simulator::suspension::{AxleSuspension, SuspensionConfig};
use crate::simulator::tire::{MagicFormulaTire, TireCoefficients};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct CarPhysicsData {
    pub name: String,
    pub mass_kg: f32,
    pub inertia_xx: f32,
    pub inertia_yy: f32,
    pub inertia_zz: f32,
    pub wheelbase: f32,
    pub cg_location: f32,
    pub track_front: f32,
    pub track_rear: f32,

    pub torque_curve: Vec<(f32, f32)>,
    pub rpm_limiter: f32,
    pub rpm_idle: f32,
    pub engine_inertia: f32,

    pub gear_ratios: Vec<f32>,
    pub final_drive: f32,

    pub brake_max_torque: f32,
    pub brake_front_share: f32,

    pub steer_lock_deg: f32,
    pub steer_ratio: f32,

    pub tire_front: TireAxleData,
    pub tire_rear: TireAxleData,

    // Aero from BodyCD / BodyCL LUT (AoA → coeff) or constants
    pub aero_cd: f32,
    pub aero_cl: f32,
    pub aero_area: f32,
    pub body_cd: Lut1D, // angle → Cd
    pub body_cl: Lut1D,

    // Suspension (simplified)
    pub susp_front: SuspensionData,
    pub susp_rear: SuspensionData,
    pub arb_front: Lut1D, // deflection → torque scale
    pub arb_rear: Lut1D,

    // ERS / hybrid
    pub ers: ErsData,

    // AWD share LUT (if present)
    pub awd_share: Lut1D,

    pub data_dir: PathBuf,
}

#[derive(Clone, Debug)]
pub struct TireAxleData {
    pub radius: f32,
    pub width: f32,
    pub angular_inertia: f32,
    pub dx_ref: f32,
    pub dy_ref: f32,
    pub fz0: f32,
    pub ls_exp_x: f32,
    pub ls_exp_y: f32,
    pub pressure_static: f32,
    pub pressure_ideal: f32,
    pub rolling_resistance_0: f32,
    pub friction_limit_angle_deg: f32,
    pub camber_static_deg: f32,
    /// Curve slip → normalized force shape (from LUT or synthesized)
    pub dx_curve: Lut1D,
    pub dy_curve: Lut1D,
}

#[derive(Clone, Debug)]
pub struct SuspensionData {
    pub spring_rate: f32, // N/m (wheel rate approx)
    pub damper: f32,      // N·s/m
    pub ride_height: f32, // m
    pub bump_lut: Lut1D,  // velocity → force scale
    pub rebound_lut: Lut1D,
}

#[derive(Clone, Debug)]
pub struct ErsData {
    pub enabled: bool,
    /// RPM → electric torque [Nm] (rear/MGU-K style)
    pub torque_lut: Lut1D,
    /// RPM → front axle electric torque [Nm]
    pub front_torque_lut: Lut1D,
    pub coast_torque_lut: Lut1D,
    pub max_front_kw: f32, // regulation ~200 kW above 190 km/h for LMH
    pub deploy_speed_kmh: f32,
}

impl Default for TireAxleData {
    fn default() -> Self {
        Self {
            radius: 0.355,
            width: 0.30,
            angular_inertia: 1.7,
            dx_ref: 1.42,
            dy_ref: 1.55,
            fz0: 3000.0,
            ls_exp_x: 0.86,
            ls_exp_y: 0.72,
            pressure_static: 22.0,
            pressure_ideal: 29.0,
            rolling_resistance_0: 6.55,
            friction_limit_angle_deg: 6.5,
            camber_static_deg: -2.5,
            dx_curve: Lut1D::default(),
            dy_curve: Lut1D::default(),
        }
    }
}

impl Default for SuspensionData {
    fn default() -> Self {
        Self {
            spring_rate: 120_000.0,
            damper: 5000.0,
            ride_height: 0.05,
            bump_lut: Lut1D::default(),
            rebound_lut: Lut1D::default(),
        }
    }
}

impl Default for ErsData {
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

impl Default for CarPhysicsData {
    fn default() -> Self {
        let mut d = Self {
            name: "Fallback".into(),
            mass_kg: 1110.0,
            inertia_xx: 1.055,
            inertia_yy: 0.52,
            inertia_zz: 3.68,
            wheelbase: 3.074,
            cg_location: 0.468,
            track_front: 1.612,
            track_rear: 1.562,
            torque_curve: vec![
                (2000.0, 393.7),
                (5000.0, 524.1),
                (7100.0, 643.9),
                (7900.0, 599.7),
            ],
            rpm_limiter: 8000.0,
            rpm_idle: 1200.0,
            engine_inertia: 0.078,
            gear_ratios: vec![2.617, 2.094, 1.718, 1.457, 1.264, 1.106, 1.015],
            final_drive: 3.196,
            brake_max_torque: 4680.0,
            brake_front_share: 0.59,
            steer_lock_deg: 270.0,
            steer_ratio: 13.6,
            tire_front: TireAxleData {
                width: 0.29,
                dx_ref: 1.42,
                dy_ref: 1.55,
                ..Default::default()
            },
            tire_rear: TireAxleData {
                width: 0.34,
                dx_ref: 1.491,
                dy_ref: 1.628,
                ..Default::default()
            },
            aero_cd: 0.55,
            aero_cl: 3.2,
            aero_area: 2.0,
            body_cd: Lut1D::default(),
            body_cl: Lut1D::default(),
            susp_front: SuspensionData {
                spring_rate: 130_000.0,
                damper: 538.0 * 10.0,
                ..Default::default()
            },
            susp_rear: SuspensionData {
                spring_rate: 140_000.0,
                damper: 551.0 * 10.0,
                ..Default::default()
            },
            arb_front: Lut1D::default(),
            arb_rear: Lut1D::default(),
            ers: ErsData::default(),
            awd_share: Lut1D::default(),
            data_dir: PathBuf::new(),
        };
        d.synthesize_missing_curves();
        d
    }
}

impl CarPhysicsData {
    pub fn load_auto() -> Self {
        let candidates = [
            "assets/car_data/data.json",
            "car_data/data.json",
            "/home/workdir/artifacts/acs_bevy/assets/car_data/data.json",
            "/home/workdir/artifacts/ac_data/data/data.json",
        ];
        for p in candidates {
            let path = Path::new(p);
            if path.exists() {
                match Self::load_from_json(path) {
                    Ok(d) => {
                        eprintln!(
                            "[car_data] {} | mass={} | ERS={} | aero_lut={} | susp bump F={}",
                            d.name,
                            d.mass_kg,
                            d.ers.enabled,
                            !d.body_cd.is_empty(),
                            !d.susp_front.bump_lut.is_empty()
                        );
                        return d;
                    }
                    Err(e) => eprintln!("[car_data] {p}: {e}"),
                }
            }
        }
        // Ultimo tentativo: cartella AC con soli .ini
        for p in [
            "assets/car_data",
            "/home/workdir/artifacts/ac_data/data",
            "/home/workdir/artifacts/ac_data",
        ] {
            if Path::new(p).is_dir() {
                if let Ok(imp) = AcImporter::from_car_folder(p) {
                    let d = imp.to_physics_data();
                    eprintln!("[car_data] imported AC ini from {p}");
                    return d;
                }
            }
        }
        eprintln!("[car_data] fallback built-in");
        Self::default()
    }

    pub fn load_from_json(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let dir = path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let mut data = Self::parse_json(&text)?;
        data.data_dir = dir.clone();
        data.load_luts(&dir);
        data.synthesize_missing_curves();
        Ok(data)
    }

    fn parse_json(text: &str) -> Result<Self, String> {
        let mut data = Self::default();
        if let Some(n) = extract_str(text, "name") {
            data.name = n;
        }
        if let Some(v) = extract_f32(text, "TOTALMASS") {
            data.mass_kg = v;
        }
        if let Some(s) = extract_str(text, "INERTIA") {
            let parts: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            if parts.len() >= 3 {
                data.inertia_xx = parts[0];
                data.inertia_yy = parts[1];
                data.inertia_zz = parts[2];
            }
        }
        if let Some(v) = extract_f32(text, "WHEELBASE") {
            data.wheelbase = v;
        }
        if let Some(v) = extract_f32(text, "CG_LOCATION") {
            data.cg_location = v;
        }
        let tracks = extract_all_f32(text, "TRACK");
        if tracks.len() >= 2 {
            data.track_front = tracks[0];
            data.track_rear = tracks[1];
        }
        if let Some(v) = extract_f32(text, "LIMITER") {
            data.rpm_limiter = v;
        }
        for v in extract_all_f32(text, "INERTIA") {
            if (0.01..1.0).contains(&v) {
                data.engine_inertia = v;
                break;
            }
        }
        let mut gears = Vec::new();
        for i in 1..=7 {
            if let Some(v) = extract_all_f32(text, &format!("GEAR_{i}")).first().copied() {
                gears.push(v);
            }
        }
        if !gears.is_empty() {
            data.gear_ratios = gears;
        }
        if let Some(v) = extract_f32(text, "FINAL") {
            data.final_drive = v;
        }
        if let Some(v) = extract_all_f32(text, "MAX_TORQUE").first().copied() {
            if v > 100.0 {
                data.brake_max_torque = v;
            }
        }
        if let Some(v) = extract_f32(text, "FRONT_SHARE") {
            data.brake_front_share = v;
        }
        if let Some(v) = extract_f32(text, "STEER_LOCK") {
            data.steer_lock_deg = v;
        }
        if let Some(v) = extract_f32(text, "STEER_RATIO") {
            data.steer_ratio = v;
        }
        if let Some(curve) = extract_torque_curve(text) {
            if curve.len() >= 3 {
                data.torque_curve = curve;
            }
        }
        data.tire_front = parse_tire_axle(text, true);
        data.tire_rear = parse_tire_axle(text, false);

        // Suspension rates from DAMP (first = front tire damp) and RATE tire
        let damps = extract_all_f32(text, "DAMP");
        if let Some(v) = damps.first() {
            data.susp_front.damper = v * 8.0; // scale AC tire damp toward wheel damper
        }
        if let Some(v) = damps.get(1) {
            data.susp_rear.damper = v * 8.0;
        }
        let rates = extract_all_f32(text, "RATE");
        // tire RATE values ~296872 are vertical stiffness
        for v in &rates {
            if *v > 50_000.0 && *v < 500_000.0 {
                data.susp_front.spring_rate = *v * 0.45;
                break;
            }
        }
        let mut found_rear = false;
        for v in &rates {
            if *v > 50_000.0 && *v < 500_000.0 {
                if found_rear {
                    data.susp_rear.spring_rate = *v * 0.45;
                    break;
                }
                found_rear = true;
            }
        }

        Ok(data)
    }

    /// Carica tutti i .lut presenti nella cartella data.
    fn load_luts(&mut self, dir: &Path) {
        // Aero
        if let Ok(l) = Lut1D::load(dir.join("BodyCD.lut")) {
            self.body_cd = l;
            // sample at 0 AoA for base Cd
            self.aero_cd = self.body_cd.sample(0.0).abs().clamp(0.2, 1.5);
        }
        if let Ok(l) = Lut1D::load(dir.join("BodyCL.lut")) {
            self.body_cl = l;
            self.aero_cl = self.body_cl.sample(0.0).abs().clamp(0.5, 8.0);
        }

        // ARB
        if let Ok(l) = Lut1D::load(dir.join("arb_f.lut")) {
            self.arb_front = l;
        }
        if let Ok(l) = Lut1D::load(dir.join("arb_r.lut")) {
            self.arb_rear = l;
        }

        // Dampers bump/rebound (usa setup 1 come default)
        if let Ok(l) = Lut1D::load(dir.join("fast_fbump1.lut")) {
            self.susp_front.bump_lut = l;
        }
        if let Ok(l) = Lut1D::load(dir.join("fast_frebound1.lut"))
            .or_else(|_| Lut1D::load(dir.join("fast_fbump1.lut")))
        {
            // rebound may be named frebound
            self.susp_front.rebound_lut = l;
        }
        // try frebound path
        for name in ["fast_frebound1.lut", "slow_frebound1.lut"] {
            if let Ok(l) = Lut1D::load(dir.join(name)) {
                self.susp_front.rebound_lut = l;
                break;
            }
        }
        if let Ok(l) = Lut1D::load(dir.join("fast_rbump1.lut")) {
            self.susp_rear.bump_lut = l;
        }
        for name in ["fast_rrebound1.lut", "slow_rrebound1.lut"] {
            if let Ok(l) = Lut1D::load(dir.join(name)) {
                self.susp_rear.rebound_lut = l;
                break;
            }
        }

        // Tire DX/DY curves if present (often external)
        for (name, target) in [
            ("F_DX_S.lut", true),
            ("F_DY_S.lut", true),
            ("R_DX_S.lut", false),
            ("R_DY_S.lut", false),
        ] {
            if let Ok(l) = Lut1D::load(dir.join(name)) {
                if name.contains("DX") {
                    if target {
                        self.tire_front.dx_curve = l;
                    } else {
                        self.tire_rear.dx_curve = l;
                    }
                } else if target {
                    self.tire_front.dy_curve = l;
                } else {
                    self.tire_rear.dy_curve = l;
                }
            }
        }

        // ERS
        let mut ers = ErsData {
            enabled: false,
            ..Default::default()
        };
        if let Ok(l) = Lut1D::load(dir.join("ers_torque.lut")) {
            ers.torque_lut = l;
            ers.enabled = true;
        }
        if let Ok(l) = Lut1D::load(dir.join("ers_front_torque.lut")) {
            ers.front_torque_lut = l;
            ers.enabled = true;
        }
        if let Ok(l) = Lut1D::load(dir.join("ers_coast_torque.lut")) {
            ers.coast_torque_lut = l;
        }
        self.ers = ers;

        if let Ok(l) = Lut1D::load(dir.join("awd_share.lut")) {
            self.awd_share = l;
        }
    }

    pub fn synthesize_missing_curves(&mut self) {
        // Tire slip curves from FRICTION_LIMIT_ANGLE + REF
        for axle in [&mut self.tire_front, &mut self.tire_rear] {
            let peak_slip = axle.friction_limit_angle_deg.to_radians().max(0.05);
            if axle.dy_curve.is_empty() {
                axle.dy_curve = synthesize_slip_curve(peak_slip, 0.35, 41);
            }
            if axle.dx_curve.is_empty() {
                // longitudinal peak slip ratio ~0.10–0.15
                axle.dx_curve = synthesize_slip_curve(0.12, 0.4, 41);
            }
        }
        if self.susp_front.bump_lut.is_empty() {
            self.susp_front.bump_lut =
                Lut1D::from_pairs(vec![(0.0, 0.0), (0.5, 500.0), (1.0, 1200.0)]);
        }
        if self.susp_rear.bump_lut.is_empty() {
            self.susp_rear.bump_lut =
                Lut1D::from_pairs(vec![(0.0, 0.0), (0.5, 500.0), (1.0, 1200.0)]);
        }
    }

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

    pub fn build_tire_front(&self) -> MagicFormulaTire {
        MagicFormulaTire::new(calibrate_mf(&self.tire_front))
    }
    pub fn build_tire_rear(&self) -> MagicFormulaTire {
        MagicFormulaTire::new(calibrate_mf(&self.tire_rear))
    }

    pub fn yaw_inertia(&self) -> f32 {
        if self.inertia_zz > 10.0 {
            self.inertia_zz
        } else {
            (self.inertia_zz * self.mass_kg).clamp(800.0, 4000.0)
        }
    }

    /// Converte nel VehicleConfig usato dal modulo physics.
    pub fn to_vehicle_config(&self) -> VehicleConfig {
        VehicleConfig {
            name: self.name.clone(),
            mass: self.mass_kg,
            wheelbase: self.wheelbase,
            cg_location: self.cg_location,
            wheel_radius: self.tire_rear.radius,
            steer_lock_deg: self.steer_lock_deg,
            steer_ratio: self.steer_ratio,
            engine: EngineConfig {
                torque_curve: self.torque_curve.clone(),
                rpm_limiter: self.rpm_limiter,
                rpm_idle: self.rpm_idle,
                inertia: self.engine_inertia,
            },
            drivetrain: DrivetrainConfig {
                gear_ratios: self.gear_ratios.clone(),
                final_drive: self.final_drive,
                efficiency: 0.92,
            },
            brakes: BrakeConfig {
                max_torque_nm: self.brake_max_torque,
                front_share: self.brake_front_share,
            },
            aero: AeroConfig {
                cd: self.aero_cd,
                cl: self.aero_cl,
                area: self.aero_area,
                air_density: 1.225,
                body_cd: self.body_cd.clone(),
                body_cl: self.body_cl.clone(),
            },
            ers: ErsConfig {
                enabled: self.ers.enabled,
                torque_lut: self.ers.torque_lut.clone(),
                front_torque_lut: self.ers.front_torque_lut.clone(),
                coast_torque_lut: self.ers.coast_torque_lut.clone(),
                max_front_kw: self.ers.max_front_kw,
                deploy_speed_kmh: self.ers.deploy_speed_kmh,
            },
            suspension: SuspensionConfig {
                front: AxleSuspension {
                    spring_rate: self.susp_front.spring_rate,
                    damper: self.susp_front.damper,
                    ride_height: self.susp_front.ride_height,
                    bump_lut: self.susp_front.bump_lut.clone(),
                    rebound_lut: self.susp_front.rebound_lut.clone(),
                },
                rear: AxleSuspension {
                    spring_rate: self.susp_rear.spring_rate,
                    damper: self.susp_rear.damper,
                    ride_height: self.susp_rear.ride_height,
                    bump_lut: self.susp_rear.bump_lut.clone(),
                    rebound_lut: self.susp_rear.rebound_lut.clone(),
                },
                arb_front: self.arb_front.clone(),
                arb_rear: self.arb_rear.clone(),
            },
            dx_ref_f: self.tire_front.dx_ref,
            dy_ref_f: self.tire_front.dy_ref,
            dx_ref_r: self.tire_rear.dx_ref,
            dy_ref_r: self.tire_rear.dy_ref,
            fz0_f: self.tire_front.fz0,
            fz0_r: self.tire_rear.fz0,
            ls_exp_x_f: self.tire_front.ls_exp_x,
            ls_exp_y_f: self.tire_front.ls_exp_y,
            ls_exp_x_r: self.tire_rear.ls_exp_x,
            ls_exp_y_r: self.tire_rear.ls_exp_y,
            pressure_f: self.tire_front.pressure_static,
            pressure_r: self.tire_rear.pressure_static,
            camber_f: self.tire_front.camber_static_deg,
            camber_r: self.tire_rear.camber_static_deg,
            rolling_res: (self.tire_front.rolling_resistance_0
                + self.tire_rear.rolling_resistance_0)
                * 0.5,
            yaw_inertia: self.yaw_inertia(),
        }
    }

    /// Aero coefficients at pitch angle [rad].
    pub fn aero_coeffs(&self, pitch_rad: f32) -> (f32, f32) {
        let deg = pitch_rad.to_degrees();
        let cd = if self.body_cd.is_empty() {
            self.aero_cd
        } else {
            self.body_cd.sample(deg).abs()
        };
        let cl = if self.body_cl.is_empty() {
            self.aero_cl
        } else {
            self.body_cl.sample(deg).abs()
        };
        (cd.clamp(0.15, 2.0), cl.clamp(0.0, 10.0))
    }

    /// Tire longitudinal force using pack curve: Fx = DX_REF * Fz_eff * shape(kappa)
    pub fn tire_fx(&self, front: bool, kappa: f32, fz: f32) -> f32 {
        let axle = if front {
            &self.tire_front
        } else {
            &self.tire_rear
        };
        let load_sens = load_sens(fz, axle.fz0, axle.ls_exp_x);
        let shape = if axle.dx_curve.is_empty() {
            kappa.signum() * (kappa.abs() / 0.12).min(1.0)
        } else {
            axle.dx_curve.sample(kappa)
        };
        axle.dx_ref * fz * load_sens * shape
    }

    /// Tire lateral force: Fy = DY_REF * Fz_eff * shape(alpha)
    pub fn tire_fy(&self, front: bool, alpha: f32, fz: f32) -> f32 {
        let axle = if front {
            &self.tire_front
        } else {
            &self.tire_rear
        };
        let load_sens = load_sens(fz, axle.fz0, axle.ls_exp_y);
        let shape = if axle.dy_curve.is_empty() {
            let peak = axle.friction_limit_angle_deg.to_radians();
            alpha.signum() * (alpha.abs() / peak.max(0.05)).min(1.0)
        } else {
            axle.dy_curve.sample(alpha)
        };
        axle.dy_ref * fz * load_sens * shape
    }

    /// ERS front motor torque [Nm] at rpm and speed (regulation gate).
    pub fn ers_front_torque(&self, rpm: f32, speed_kmh: f32, throttle: f32) -> f32 {
        if !self.ers.enabled || throttle < 0.05 {
            return 0.0;
        }
        if speed_kmh < self.ers.deploy_speed_kmh {
            return 0.0;
        }
        let tq = if self.ers.front_torque_lut.is_empty() {
            // 200 kW @ wheel ≈ 200000 / omega
            let omega = (rpm * 0.1).max(50.0) * std::f32::consts::TAU / 60.0;
            (self.ers.max_front_kw * 1000.0 / omega).min(800.0)
        } else {
            self.ers.front_torque_lut.sample(rpm)
        };
        tq * throttle
    }

    /// Damper force [N] from LUT + base coefficient, sign = velocity of compression.
    pub fn damper_force(&self, front: bool, vel: f32) -> f32 {
        let sus = if front {
            &self.susp_front
        } else {
            &self.susp_rear
        };
        let base = sus.damper * vel;
        if vel >= 0.0 {
            // bump
            if sus.bump_lut.is_empty() {
                base
            } else {
                sus.bump_lut.sample(vel.abs()) * vel.signum()
            }
        } else if sus.rebound_lut.is_empty() {
            base
        } else {
            -sus.rebound_lut.sample(vel.abs())
        }
    }
}

fn load_sens(fz: f32, fz0: f32, exp: f32) -> f32 {
    let r = (fz / fz0.max(1.0)).max(0.1);
    r.powf(exp.clamp(0.4, 1.2)).clamp(0.5, 1.3)
}

fn calibrate_mf(axle: &TireAxleData) -> TireCoefficients {
    let mut c = TireCoefficients::slick();
    let fz0 = axle.fz0.max(500.0);
    let fz_kn = fz0 * 0.001;
    let target_dy = axle.dy_ref * fz0;
    let target_dx = axle.dx_ref * fz0;
    c.a1 = -5.0;
    c.a2 = (target_dy / fz_kn - c.a1 * fz_kn).max(100.0);
    c.b1 = -5.0;
    c.b2 = (target_dx / fz_kn - c.b1 * fz_kn).max(100.0);
    c.a3 = c.a2 * 1.1;
    c.nominal_load_n = fz0;
    c.load_sens_exp = (axle.ls_exp_y + axle.ls_exp_x) * 0.5;
    c.opt_pressure_psi = axle.pressure_ideal;
    c
}

fn parse_tire_axle(text: &str, front: bool) -> TireAxleData {
    let mut t = TireAxleData::default();
    let i = if front { 0 } else { 1 };
    let get = |key: &str| extract_all_f32(text, key).get(i).copied();
    if let Some(v) = get("DY_REF") {
        t.dy_ref = v;
    }
    if let Some(v) = get("DX_REF") {
        t.dx_ref = v;
    }
    if let Some(v) = get("RADIUS") {
        t.radius = v;
    }
    let widths: Vec<f32> = extract_all_f32(text, "WIDTH")
        .into_iter()
        .filter(|w| *w < 1.0)
        .collect();
    if let Some(v) = widths.get(i) {
        t.width = *v;
    }
    if let Some(v) = get("FZ0") {
        t.fz0 = v;
    }
    if let Some(v) = get("LS_EXPY") {
        t.ls_exp_y = v;
    }
    if let Some(v) = get("LS_EXPX") {
        t.ls_exp_x = v;
    }
    if let Some(v) = get("PRESSURE_STATIC") {
        t.pressure_static = v;
    }
    if let Some(v) = get("PRESSURE_IDEAL") {
        t.pressure_ideal = v;
    }
    if let Some(v) = get("ANGULAR_INERTIA") {
        t.angular_inertia = v;
    }
    if let Some(v) = get("ROLLING_RESISTANCE_0") {
        t.rolling_resistance_0 = v;
    }
    if let Some(v) = get("FRICTION_LIMIT_ANGLE") {
        t.friction_limit_angle_deg = v;
    }
    if let Some(v) = extract_all_f32(text, "STATIC_CAMBER").get(i).copied() {
        t.camber_static_deg = v;
    }
    t
}

fn extract_str(text: &str, key: &str) -> Option<String> {
    let pat = format!("\"{key}\"");
    let idx = text.find(&pat)?;
    let after = text[idx + pat.len()..]
        .trim_start()
        .trim_start_matches(':')
        .trim_start();
    let after = after.strip_prefix('"')?;
    let end = after.find('"')?;
    Some(after[..end].to_string())
}

fn extract_f32(text: &str, key: &str) -> Option<f32> {
    extract_all_f32(text, key).into_iter().next()
}

fn extract_all_f32(text: &str, key: &str) -> Vec<f32> {
    let pat = format!("\"{key}\"");
    let mut out = Vec::new();
    let mut search = text;
    while let Some(idx) = search.find(&pat) {
        let after = search[idx + pat.len()..]
            .trim_start()
            .trim_start_matches(':')
            .trim_start();
        let s = if let Some(stripped) = after.strip_prefix('"') {
            stripped.split('"').next().unwrap_or("")
        } else {
            after.split([',', '}', '\n', ' ']).next().unwrap_or("")
        };
        if let Ok(v) = s.trim().parse::<f32>() {
            out.push(v);
        }
        search = &search[idx + pat.len()..];
    }
    out
}

fn extract_torque_curve(text: &str) -> Option<Vec<(f32, f32)>> {
    let start = text.find("\"torqueCurve\"")?;
    let arr_start = text[start..].find('[')? + start;
    let mut depth = 0i32;
    let mut end = arr_start;
    for (i, ch) in text[arr_start..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end = arr_start + i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    let arr = &text[arr_start..end];
    let mut nums = Vec::new();
    for part in arr.split(['[', ']', ',']) {
        if let Ok(v) = part.trim().parse::<f32>() {
            nums.push(v);
        }
    }
    let mut curve = Vec::new();
    for chunk in nums.chunks(2) {
        if chunk.len() == 2 {
            curve.push((chunk[0], chunk[1]));
        }
    }
    if curve.is_empty() {
        None
    } else {
        Some(curve)
    }
}
