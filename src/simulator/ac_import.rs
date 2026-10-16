//! Import Assetto Corsa → RacingEdge
//!
//! Legge cartelle auto AC classiche (`data/*.ini` + `.lut`) e le converte
//! nello stesso schema JSON già usato da `car_data` (come il pack fornito).
//!
//! Uso:
//! ```ignore
//! let data = AcImporter::from_car_folder("path/to/cars/ferrari_499p")?
//!     .to_physics_data();
//! // oppure scrivi JSON:
//! AcImporter::from_car_folder(path)?.write_json("assets/car_data/data.json")?;
//! ```

use crate::simulator::car_data::{CarPhysicsData, ErsData, SuspensionData, TireAxleData};
use crate::simulator::lut::Lut1D;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Sezione INI: chiavi case-insensitive (AC usa KEY=VALUE).
#[derive(Debug, Default, Clone)]
pub struct IniSection {
    pub values: HashMap<String, String>,
}

impl IniSection {
    pub fn get_f32(&self, key: &str) -> Option<f32> {
        self.values
            .get(&key.to_ascii_uppercase())
            .and_then(|s| parse_f32_loose(s))
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.values
            .get(&key.to_ascii_uppercase())
            .map(|s| s.as_str())
    }
}

/// File INI AC intero.
#[derive(Debug, Default, Clone)]
pub struct IniFile {
    pub sections: HashMap<String, IniSection>,
}

impl IniFile {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let text = fs::read_to_string(path.as_ref())
            .map_err(|e| format!("{}: {e}", path.as_ref().display()))?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut file = IniFile::default();
        let mut current = String::from("_ROOT");
        file.sections.insert(current.clone(), IniSection::default());

        for raw in text.lines() {
            let line = raw.split(';').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                current = line[1..line.len() - 1].trim().to_ascii_uppercase();
                file.sections
                    .entry(current.clone())
                    .or_insert_with(IniSection::default);
                continue;
            }
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim().to_ascii_uppercase();
                let val = v.trim().to_string();
                file.sections
                    .entry(current.clone())
                    .or_insert_with(IniSection::default)
                    .values
                    .insert(key, val);
            }
        }
        Ok(file)
    }

    pub fn section(&self, name: &str) -> Option<&IniSection> {
        self.sections.get(&name.to_ascii_uppercase())
    }

    pub fn get_f32(&self, section: &str, key: &str) -> Option<f32> {
        self.section(section)?.get_f32(key)
    }

    pub fn get_str(&self, section: &str, key: &str) -> Option<&str> {
        self.section(section)?.get_str(key)
    }
}

fn parse_f32_loose(s: &str) -> Option<f32> {
    let s = s.trim().trim_matches('"');
    // AC sometimes uses comma decimals
    let s = s.replace(',', ".");
    s.parse().ok()
}

/// Importatore cartella auto AC.
#[derive(Debug)]
pub struct AcImporter {
    pub car_dir: PathBuf,
    pub data_dir: PathBuf,
    pub name: String,
    pub files: HashMap<String, IniFile>,
}

impl AcImporter {
    /// `car_dir` = cartella auto (`.../content/cars/ferrari_499p`)
    /// oppure direttamente la cartella `data/`.
    pub fn from_car_folder(car_dir: impl AsRef<Path>) -> Result<Self, String> {
        let car_dir = car_dir.as_ref().to_path_buf();
        let data_dir = if car_dir.join("data").is_dir() {
            car_dir.join("data")
        } else if car_dir.file_name().map(|n| n == "data").unwrap_or(false) {
            car_dir.clone()
        } else if car_dir.join("car.ini").exists() || car_dir.join("engine.ini").exists() {
            car_dir.clone()
        } else {
            return Err(format!(
                "cartella AC non riconosciuta: {} (serve data/ o *.ini)",
                car_dir.display()
            ));
        };

        let name = car_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("imported_car")
            .to_string();

        let mut files = HashMap::new();
        let entries = fs::read_dir(&data_dir)
            .map_err(|e| format!("read_dir {}: {e}", data_dir.display()))?;
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|x| x.to_str()) == Some("ini") {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_ascii_lowercase();
                match IniFile::load(&path) {
                    Ok(ini) => {
                        files.insert(stem, ini);
                    }
                    Err(err) => eprintln!("[ac_import] skip {}: {err}", path.display()),
                }
            }
        }

        if files.is_empty() {
            // forse solo data.json già convertito
            if data_dir.join("data.json").exists() {
                return Err(
                    "solo data.json presente: usa CarPhysicsData::load_from_json".into(),
                );
            }
            return Err(format!("nessun .ini in {}", data_dir.display()));
        }

        eprintln!(
            "[ac_import] {} — {} file .ini da {}",
            name,
            files.len(),
            data_dir.display()
        );

        Ok(Self {
            car_dir,
            data_dir,
            name,
            files,
        })
    }

    fn ini(&self, name: &str) -> Option<&IniFile> {
        self.files.get(name)
    }

    /// Converte in `CarPhysicsData` (stesso tipo del pack JSON).
    pub fn to_physics_data(&self) -> CarPhysicsData {
        let mut d = CarPhysicsData::default();
        d.name = self.name.clone();
        d.data_dir = self.data_dir.clone();

        // --- car.ini ---
        if let Some(car) = self.ini("car") {
            if let Some(v) = car.get_f32("BASIC", "TOTALMASS").or_else(|| car.get_f32("INFO", "TOTALMASS")) {
                d.mass_kg = v;
            }
            if let Some(v) = car.get_f32("BASIC", "INERTIA") {
                d.inertia_zz = v;
            }
            // GRAPHICS / BASIC wheelbase sometimes elsewhere
        }

        // --- drivetrain.ini ---
        if let Some(dt) = self.ini("drivetrain") {
            let mut gears = Vec::new();
            for i in 1..=8 {
                if let Some(v) = dt.get_f32("GEARS", &format!("GEAR_{i}")) {
                    gears.push(v);
                }
            }
            if !gears.is_empty() {
                d.gear_ratios = gears;
            }
            if let Some(v) = dt.get_f32("GEARS", "FINAL") {
                d.final_drive = v;
            }
        }

        // --- engine.ini ---
        if let Some(eng) = self.ini("engine") {
            if let Some(v) = eng.get_f32("ENGINE_DATA", "LIMITER") {
                d.rpm_limiter = v;
            }
            if let Some(v) = eng.get_f32("ENGINE_DATA", "MINIMUM") {
                // idle-ish
                if v > 0.0 && v < 3000.0 {
                    d.rpm_idle = v.max(800.0);
                }
            }
            if let Some(v) = eng.get_f32("ENGINE_DATA", "INERTIA") {
                d.engine_inertia = v;
            }
            // POWER_CURVE → torque.lut
            if let Some(curve_name) = eng
                .get_str("ENGINE_DATA", "POWER_CURVE")
                .or_else(|| eng.get_str("HEADER", "POWER_CURVE"))
            {
                let lut_path = self.data_dir.join(curve_name);
                if let Ok(lut) = Lut1D::load(&lut_path) {
                    d.torque_curve = lut.points;
                }
            }
        }

        // fallback: torque.lut generico
        if d.torque_curve.len() < 3 {
            for name in ["torque.lut", "power.lut"] {
                if let Ok(lut) = Lut1D::load(self.data_dir.join(name)) {
                    d.torque_curve = lut.points;
                    break;
                }
            }
        }

        // --- brakes.ini ---
        if let Some(br) = self.ini("brakes") {
            if let Some(v) = br.get_f32("DATA", "MAX_TORQUE") {
                d.brake_max_torque = v;
            }
            if let Some(v) = br.get_f32("DATA", "FRONT_SHARE") {
                d.brake_front_share = v;
            }
        }

        // --- tyres.ini ---
        if let Some(ty) = self.ini("tyres") {
            d.tire_front = parse_tyre_section(ty, "FRONT");
            d.tire_rear = parse_tyre_section(ty, "REAR");
            // thermal compounds sometimes named differently
            if d.tire_front.dy_ref <= 0.1 {
                if let Some(sec) = first_section_with(ty, "DY_REF") {
                    d.tire_front = parse_tyre_section(ty, &sec);
                }
            }
        }

        // --- suspension / basic dimensions from suspension.ini or car.ini ---
        if let Some(sus) = self.ini("suspensions").or_else(|| self.ini("suspension")) {
            if let Some(v) = sus.get_f32("BASIC", "WHEELBASE") {
                d.wheelbase = v;
            }
            if let Some(v) = sus.get_f32("BASIC", "CG_LOCATION") {
                d.cg_location = v;
            }
            if let Some(v) = sus.get_f32("FRONT", "TRACK") {
                d.track_front = v;
            }
            if let Some(v) = sus.get_f32("REAR", "TRACK") {
                d.track_rear = v;
            }
            if let Some(v) = sus.get_f32("FRONT", "BASEY") {
                d.susp_front.ride_height = v.abs();
            }
            if let Some(v) = sus.get_f32("REAR", "BASEY") {
                d.susp_rear.ride_height = v.abs();
            }
        }

        // damper / spring from tyres RATE DAMP if present
        if let Some(ty) = self.ini("tyres") {
            if let Some(v) = ty.get_f32("FRONT", "DAMP").or_else(|| ty.get_f32("FRONT", "RATE")) {
                if v < 10_000.0 {
                    d.susp_front.damper = v * 8.0;
                } else {
                    d.susp_front.spring_rate = v * 0.45;
                }
            }
            if let Some(v) = ty.get_f32("REAR", "DAMP") {
                d.susp_rear.damper = v * 8.0;
            }
            if let Some(v) = ty.get_f32("FRONT", "RATE") {
                if v > 50_000.0 {
                    d.susp_front.spring_rate = v * 0.45;
                }
            }
            if let Some(v) = ty.get_f32("REAR", "RATE") {
                if v > 50_000.0 {
                    d.susp_rear.spring_rate = v * 0.45;
                }
            }
        }

        // steer from car.ini CONTROLS
        if let Some(car) = self.ini("car") {
            if let Some(v) = car.get_f32("CONTROLS", "STEER_LOCK") {
                d.steer_lock_deg = v;
            }
            if let Some(v) = car.get_f32("CONTROLS", "STEER_RATIO") {
                d.steer_ratio = v;
            }
        }

        // aero.ini
        if let Some(aero) = self.ini("aero") {
            if let Some(v) = aero.get_f32("DATA", "CD").or_else(|| aero.get_f32("FRONT", "CD")) {
                d.aero_cd = v;
            }
            if let Some(v) = aero.get_f32("DATA", "CL").or_else(|| aero.get_f32("REAR", "CL_GAIN")) {
                d.aero_cl = v.abs();
            }
        }

        // LUT aero BodyCD/CL
        if let Ok(l) = Lut1D::load(self.data_dir.join("BodyCD.lut")) {
            d.body_cd = l;
            d.aero_cd = d.body_cd.sample(0.0).abs().clamp(0.2, 1.5);
        }
        if let Ok(l) = Lut1D::load(self.data_dir.join("BodyCL.lut")) {
            d.body_cl = l;
            d.aero_cl = d.body_cl.sample(0.0).abs().clamp(0.5, 8.0);
        }

        // ARB + dampers LUT
        if let Ok(l) = Lut1D::load(self.data_dir.join("arb_f.lut")) {
            d.arb_front = l;
        }
        if let Ok(l) = Lut1D::load(self.data_dir.join("arb_r.lut")) {
            d.arb_rear = l;
        }
        if let Ok(l) = Lut1D::load(self.data_dir.join("fast_fbump1.lut")) {
            d.susp_front.bump_lut = l;
        }
        if let Ok(l) = Lut1D::load(self.data_dir.join("fast_frebound1.lut")) {
            d.susp_front.rebound_lut = l;
        }
        if let Ok(l) = Lut1D::load(self.data_dir.join("fast_rbump1.lut")) {
            d.susp_rear.bump_lut = l;
        }

        // ERS
        let mut ers = ErsData::default();
        if let Ok(l) = Lut1D::load(self.data_dir.join("ers_torque.lut")) {
            ers.torque_lut = l;
            ers.enabled = true;
        }
        if let Ok(l) = Lut1D::load(self.data_dir.join("ers_front_torque.lut")) {
            ers.front_torque_lut = l;
            ers.enabled = true;
        }
        if let Ok(l) = Lut1D::load(self.data_dir.join("ers_coast_torque.lut")) {
            ers.coast_torque_lut = l;
        }
        d.ers = ers;

        if let Ok(l) = Lut1D::load(self.data_dir.join("awd_share.lut")) {
            d.awd_share = l;
        }

        d.synthesize_missing_curves();
        d
    }

    /// Esporta JSON compatibile RacingEdge (stesso schema del pack fornito, semplificato).
    pub fn write_json(&self, out: impl AsRef<Path>) -> Result<(), String> {
        let d = self.to_physics_data();
        let json = physics_to_json(&d);
        if let Some(parent) = out.as_ref().parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(out.as_ref(), json).map_err(|e| e.to_string())?;
        eprintln!("[ac_import] wrote {}", out.as_ref().display());
        Ok(())
    }

    /// Import + copia LUT utili in `assets/car_data/`.
    pub fn install_as_assets(&self, assets_car_data: impl AsRef<Path>) -> Result<CarPhysicsData, String> {
        let dest = assets_car_data.as_ref();
        fs::create_dir_all(dest).map_err(|e| e.to_string())?;
        self.write_json(dest.join("data.json"))?;

        // copia lut rilevanti
        if let Ok(entries) = fs::read_dir(&self.data_dir) {
            for e in entries.flatten() {
                let p = e.path();
                let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
                if matches!(ext, "lut" | "2dlut" | "2Dlut") {
                    if let Some(name) = p.file_name() {
                        let _ = fs::copy(&p, dest.join(name));
                    }
                }
            }
        }
        CarPhysicsData::load_from_json(dest.join("data.json"))
    }
}

fn parse_tyre_section(ty: &IniFile, section: &str) -> TireAxleData {
    let mut t = TireAxleData::default();
    if let Some(v) = ty.get_f32(section, "RADIUS") {
        t.radius = v;
    }
    if let Some(v) = ty.get_f32(section, "WIDTH") {
        t.width = v;
    }
    if let Some(v) = ty.get_f32(section, "ANGULAR_INERTIA") {
        t.angular_inertia = v;
    }
    if let Some(v) = ty.get_f32(section, "DX_REF") {
        t.dx_ref = v;
    }
    if let Some(v) = ty.get_f32(section, "DY_REF") {
        t.dy_ref = v;
    }
    if let Some(v) = ty.get_f32(section, "FZ0") {
        t.fz0 = v;
    }
    if let Some(v) = ty.get_f32(section, "LS_EXPX") {
        t.ls_exp_x = v;
    }
    if let Some(v) = ty.get_f32(section, "LS_EXPY") {
        t.ls_exp_y = v;
    }
    if let Some(v) = ty.get_f32(section, "PRESSURE_STATIC") {
        t.pressure_static = v;
    }
    if let Some(v) = ty.get_f32(section, "PRESSURE_IDEAL") {
        t.pressure_ideal = v;
    }
    if let Some(v) = ty.get_f32(section, "ROLLING_RESISTANCE_0") {
        t.rolling_resistance_0 = v;
    }
    if let Some(v) = ty.get_f32(section, "FRICTION_LIMIT_ANGLE") {
        t.friction_limit_angle_deg = v;
    }
    t
}

fn first_section_with(ini: &IniFile, key: &str) -> Option<String> {
    let key = key.to_ascii_uppercase();
    for (name, sec) in &ini.sections {
        if sec.values.contains_key(&key) {
            return Some(name.clone());
        }
    }
    None
}

fn physics_to_json(d: &CarPhysicsData) -> String {
    let mut torque = String::from("[\n");
    for (i, (rpm, nm)) in d.torque_curve.iter().enumerate() {
        if i > 0 {
            torque.push_str(",\n");
        }
        torque.push_str(&format!("    [{rpm}, {nm}]"));
    }
    torque.push_str("\n  ]");

    let gears: Vec<String> = d
        .gear_ratios
        .iter()
        .enumerate()
        .map(|(i, g)| format!("    \"GEAR_{}\": {g}", i + 1))
        .collect();

    format!(
        r#"{{
  "name": "{}",
  "source": "assetto_corsa_import",
  "TOTALMASS": {},
  "WHEELBASE": {},
  "CG_LOCATION": {},
  "LIMITER": {},
  "FINAL": {},
  "MAX_TORQUE": {},
  "FRONT_SHARE": {},
  "STEER_LOCK": {},
  "STEER_RATIO": {},
  "INERTIA": "{},{},{}",
  "TRACK": [{}, {}],
  "GEARS": {{
{}
  }},
  "torqueCurve": {},
  "tyres": {{
    "FRONT": {{
      "RADIUS": {},
      "WIDTH": {},
      "DX_REF": {},
      "DY_REF": {},
      "FZ0": {},
      "LS_EXPX": {},
      "LS_EXPY": {},
      "PRESSURE_STATIC": {},
      "PRESSURE_IDEAL": {},
      "ANGULAR_INERTIA": {},
      "ROLLING_RESISTANCE_0": {},
      "FRICTION_LIMIT_ANGLE": {}
    }},
    "REAR": {{
      "RADIUS": {},
      "WIDTH": {},
      "DX_REF": {},
      "DY_REF": {},
      "FZ0": {},
      "LS_EXPX": {},
      "LS_EXPY": {},
      "PRESSURE_STATIC": {},
      "PRESSURE_IDEAL": {},
      "ANGULAR_INERTIA": {},
      "ROLLING_RESISTANCE_0": {},
      "FRICTION_LIMIT_ANGLE": {}
    }}
  }},
  "aero": {{
    "CD": {},
    "CL": {},
    "AREA": {}
  }},
  "ers_enabled": {}
}}
"#,
        d.name,
        d.mass_kg,
        d.wheelbase,
        d.cg_location,
        d.rpm_limiter,
        d.final_drive,
        d.brake_max_torque,
        d.brake_front_share,
        d.steer_lock_deg,
        d.steer_ratio,
        d.inertia_xx,
        d.inertia_yy,
        d.inertia_zz,
        d.track_front,
        d.track_rear,
        gears.join(",\n"),
        torque,
        d.tire_front.radius,
        d.tire_front.width,
        d.tire_front.dx_ref,
        d.tire_front.dy_ref,
        d.tire_front.fz0,
        d.tire_front.ls_exp_x,
        d.tire_front.ls_exp_y,
        d.tire_front.pressure_static,
        d.tire_front.pressure_ideal,
        d.tire_front.angular_inertia,
        d.tire_front.rolling_resistance_0,
        d.tire_front.friction_limit_angle_deg,
        d.tire_rear.radius,
        d.tire_rear.width,
        d.tire_rear.dx_ref,
        d.tire_rear.dy_ref,
        d.tire_rear.fz0,
        d.tire_rear.ls_exp_x,
        d.tire_rear.ls_exp_y,
        d.tire_rear.pressure_static,
        d.tire_rear.pressure_ideal,
        d.tire_rear.angular_inertia,
        d.tire_rear.rolling_resistance_0,
        d.tire_rear.friction_limit_angle_deg,
        d.aero_cd,
        d.aero_cl,
        d.aero_area,
        d.ers.enabled,
    )
}

/// Rileva se un path è pack JSON, cartella AC con ini, o data.json singolo.
pub fn load_car_auto(path: impl AsRef<Path>) -> Result<CarPhysicsData, String> {
    let path = path.as_ref();
    if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("json") {
        return CarPhysicsData::load_from_json(path);
    }
    if path.is_dir() {
        if path.join("data.json").exists() {
            return CarPhysicsData::load_from_json(path.join("data.json"));
        }
        if path.join("data/data.json").exists() {
            return CarPhysicsData::load_from_json(path.join("data/data.json"));
        }
        // AC classic
        let importer = AcImporter::from_car_folder(path)?;
        return Ok(importer.to_physics_data());
    }
    Err(format!("path non valido: {}", path.display()))
}
