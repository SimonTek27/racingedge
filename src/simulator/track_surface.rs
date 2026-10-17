//! Superficie di pista — griglia di grip dinamica (gomma, palline di gomma,
//! bagnato) e modelli di sezione, banking ed elevazione.
//!
//! Port di ksengine `TrackSurface.h` e `TrackPhysics.*` (le parti Qt-free).
//! L'originale usa un singleton (`TrackSurface::instance()`); qui è una struct
//! normale, così l'app la possiede come risorsa senza stato globale condiviso.

use std::collections::HashMap;

use bevy::math::Vec3;

/// Campione di superficie in un punto.
#[derive(Clone, Copy, Debug)]
pub struct SurfaceSample {
    pub grip: f32,
    pub wetness: f32,
    pub marbles: f32,
    pub temperature: f32,
    pub rubber: f32,
}

impl Default for SurfaceSample {
    fn default() -> Self {
        Self {
            grip: 1.0,
            wetness: 0.0,
            marbles: 0.0,
            temperature: 25.0,
            rubber: 0.0,
        }
    }
}

/// Griglia spaziale di grip (port di `TrackSurface`).
#[derive(Clone, Debug)]
pub struct TrackSurface {
    res: i32,
    world_size: f32,
    base_grip: f32,
    global_wet: f32,
    global_rubber: f32,
    base_temp: f32,
    off_track_grip: f32,
    kerb_grip: f32,
    materials: HashMap<String, f32>,
    rubber: Vec<f32>,
    wet: Vec<f32>,
    marbles: Vec<f32>,
    temp: Vec<f32>,
}

impl Default for TrackSurface {
    fn default() -> Self {
        let mut s = Self {
            res: 256,
            world_size: 2000.0,
            base_grip: 1.0,
            global_wet: 0.0,
            global_rubber: 0.0,
            base_temp: 25.0,
            off_track_grip: 0.55,
            kerb_grip: 0.95,
            materials: HashMap::new(),
            rubber: Vec::new(),
            wet: Vec::new(),
            marbles: Vec::new(),
            temp: Vec::new(),
        };
        s.configure(256, 2000.0);
        s
    }
}

impl TrackSurface {
    /// Nuova griglia con i valori predefiniti (port del costruttore).
    pub fn new() -> Self {
        Self::default()
    }

    /// (Ri)configura risoluzione e dimensione del mondo (port di `configure`).
    pub fn configure(&mut self, res: i32, world_size: f32) {
        self.res = res.clamp(32, 512);
        self.world_size = world_size;
        let n = (self.res * self.res) as usize;
        self.rubber = vec![0.0; n];
        self.wet = vec![self.global_wet; n];
        self.marbles = vec![0.0; n];
        self.temp = vec![self.base_temp; n];
    }

    pub fn set_base_grip(&mut self, g: f32) {
        self.base_grip = g.clamp(0.5, 1.5);
    }

    pub fn set_wetness(&mut self, w: f32) {
        self.global_wet = w.clamp(0.0, 1.0);
        self.wet.fill(self.global_wet);
    }

    pub fn set_rubber(&mut self, r: f32) {
        self.global_rubber = r.clamp(0.0, 1.0);
    }

    pub fn set_temperature(&mut self, t: f32) {
        self.base_temp = t;
        self.temp.fill(t);
    }

    pub fn set_material_grip(&mut self, key: &str, friction: f32) {
        self.materials
            .insert(key.to_uppercase(), friction.clamp(0.05, 2.0));
    }

    pub fn material_grip(&self, key: &str) -> f32 {
        self.materials
            .get(&key.to_uppercase())
            .copied()
            .unwrap_or(self.base_grip)
    }

    pub fn set_off_track_grip(&mut self, g: f32) {
        self.off_track_grip = g.clamp(0.05, 1.0);
    }

    pub fn set_kerb_grip(&mut self, g: f32) {
        self.kerb_grip = g.clamp(0.2, 1.5);
    }

    pub fn off_track_grip(&self) -> f32 {
        self.off_track_grip
    }

    pub fn kerb_grip(&self) -> f32 {
        self.kerb_grip
    }

    /// Campiona la superficie in un punto (port di `sample`).
    pub fn sample(&self, world_pos: Vec3) -> SurfaceSample {
        let idx = self.cell_index(world_pos);
        let (rubber, wet, marbles, temp) = match idx {
            Some(i) => (self.rubber[i], self.wet[i], self.marbles[i], self.temp[i]),
            None => (self.global_rubber, self.global_wet, 0.0, self.base_temp),
        };
        let base = if idx.is_some() {
            self.base_grip
        } else {
            self.off_track_grip
        };
        let mut s = SurfaceSample {
            rubber: (rubber + self.global_rubber).clamp(0.0, 1.0),
            wetness: wet,
            marbles,
            temperature: temp,
            grip: 0.0,
        };
        s.grip = base * (1.0 - wet * 0.45) * (1.0 + s.rubber * 0.12) * (1.0 - marbles * 0.15);
        s.grip = s.grip.clamp(0.05, 2.0);
        s
    }

    pub fn grip(&self, p: Vec3) -> f32 {
        self.sample(p).grip
    }

    /// Deposita gomma in un'area (port di `depositRubber`).
    pub fn deposit_rubber(&mut self, p: Vec3, amount: f32, radius: f32) {
        for (idx, w) in self.cells_in_radius(p, radius) {
            self.rubber[idx] = (self.rubber[idx] + amount * w).clamp(0.0, 1.0);
        }
    }

    /// Aggiunge palline di gomma in un'area (port di `addMarbles`).
    pub fn add_marbles(&mut self, p: Vec3, amount: f32, radius: f32) {
        for (idx, w) in self.cells_in_radius(p, radius) {
            self.marbles[idx] = (self.marbles[idx] + amount * w).clamp(0.0, 1.0);
        }
    }

    /// Sincronizza bagnato e temperatura dal meteo (port di `syncFromWeather`).
    pub fn sync_from_weather(&mut self, track_wetness: f32, track_temp: f32) {
        self.set_wetness(track_wetness);
        self.set_temperature(track_temp);
    }

    /// Indice di cella per una posizione, o `None` fuori dal mondo configurato.
    fn cell_index(&self, p: Vec3) -> Option<usize> {
        if self.res <= 0 {
            return None;
        }
        let half = self.world_size * 0.5;
        let u = (p.x + half) / self.world_size;
        let v = (p.z + half) / self.world_size;
        if !(0.0..1.0).contains(&u) || !(0.0..1.0).contains(&v) {
            return None;
        }
        let ix = ((u * self.res as f32) as i32).clamp(0, self.res - 1);
        let iy = ((v * self.res as f32) as i32).clamp(0, self.res - 1);
        Some((iy * self.res + ix) as usize)
    }

    /// Celle entro `radius` da `p`, con peso lineare 1 al centro e 0 al bordo.
    fn cells_in_radius(&self, p: Vec3, radius: f32) -> Vec<(usize, f32)> {
        let mut out = Vec::new();
        if self.res <= 0 {
            return out;
        }
        let Some(center) = self.cell_index(p) else {
            return out;
        };
        let cell = self.world_size / self.res as f32;
        let r_cells = ((radius / cell) as i32 + 1).max(1);
        let cx0 = center % self.res as usize;
        let cy0 = center / self.res as usize;

        for dy in -r_cells..=r_cells {
            for dx in -r_cells..=r_cells {
                let ix = cx0 as i32 + dx;
                let iy = cy0 as i32 + dy;
                if ix < 0 || iy < 0 || ix >= self.res || iy >= self.res {
                    continue;
                }
                let wx =
                    (ix as f32 + 0.5) / self.res as f32 * self.world_size - self.world_size * 0.5;
                let wz =
                    (iy as f32 + 0.5) / self.res as f32 * self.world_size - self.world_size * 0.5;
                // Distanza dal *box* della cella, non dal centro: con celle
                // grandi e raggi piccoli il test sul centro salterebbe la cella
                // su cui poggia lo pneumatico.
                let ex = ((wx - p.x).abs() - cell * 0.5).max(0.0);
                let ez = ((wz - p.z).abs() - cell * 0.5).max(0.0);
                let d = (ex * ex + ez * ez).sqrt();
                if d > radius {
                    continue;
                }
                out.push(((iy * self.res + ix) as usize, 1.0 - d / radius));
            }
        }
        out
    }
}

// =============================================================================
// MODELLO A SEZIONI (TrackSurfaceModel)
// =============================================================================

/// Proprietà di una superficie (port di `SurfaceProperties`).
#[derive(Clone, Copy, Debug)]
pub struct SurfaceProperties {
    pub grip_coefficient: f32,
    pub rolling_resistance: f32,
    pub surface_roughness: f32,
    pub surface_temperature: f32,
    pub abrasiveness: f32,
    pub is_wet: bool,
    pub is_curb: bool,
    pub is_gravel: bool,
    pub is_grass: bool,
}

impl Default for SurfaceProperties {
    fn default() -> Self {
        Self {
            grip_coefficient: 1.0,
            rolling_resistance: 0.015,
            surface_roughness: 0.0,
            surface_temperature: 30.0,
            abrasiveness: 0.1,
            is_wet: false,
            is_curb: false,
            is_gravel: false,
            is_grass: false,
        }
    }
}

/// Modificatore di grip calcolato (port di `SurfaceGripModifier`).
#[derive(Clone, Copy, Debug)]
pub struct SurfaceGripModifier {
    pub temperature_effect: f32,
    pub wetness_reduction: f32,
    pub roughness_effect: f32,
    pub curb_bump: f32,
    pub total_grip_modifier: f32,
}

impl Default for SurfaceGripModifier {
    fn default() -> Self {
        Self {
            temperature_effect: 1.0,
            wetness_reduction: 0.0,
            roughness_effect: 1.0,
            curb_bump: 0.0,
            total_grip_modifier: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct SurfaceSection {
    start_dist: f32,
    end_dist: f32,
    properties: SurfaceProperties,
}

/// Modello di pista a sezioni di superficie (port di `TrackSurfaceModel`).
#[derive(Clone, Debug, Default)]
pub struct TrackSurfaceModel {
    sections: Vec<SurfaceSection>,
}

impl TrackSurfaceModel {
    pub fn new() -> Self {
        Self::default()
    }

    /// Proprietà della superficie alla distanza data (port di `getSurface`).
    pub fn surface_at(&self, track_position: f32) -> SurfaceProperties {
        for s in &self.sections {
            if track_position >= s.start_dist && track_position < s.end_dist {
                return s.properties;
            }
        }
        SurfaceProperties::default()
    }

    /// Registra una sezione [start, end).
    pub fn add_surface(&mut self, start_dist: f32, end_dist: f32, props: SurfaceProperties) {
        self.sections.push(SurfaceSection {
            start_dist,
            end_dist,
            properties: props,
        });
    }

    /// Modificatore di grip (port di `calculateGripModifier`).
    pub fn calculate_grip_modifier(
        &self,
        surface: &SurfaceProperties,
        tire_temp: f32,
        _ambient_temp: f32,
        _track_temp: f32,
    ) -> SurfaceGripModifier {
        let optimal_tire_temp = 80.0;
        let temp_diff = tire_temp - optimal_tire_temp;
        let temperature_effect = (1.0 - 0.002 * temp_diff * temp_diff).clamp(0.5, 1.0);
        let wetness_reduction = if surface.is_wet { 0.3 } else { 0.0 };
        let roughness_effect = 1.0 - surface.surface_roughness * 0.1;
        let curb_bump = if surface.is_curb { 0.05 } else { 0.0 };
        let total_grip_modifier = surface.grip_coefficient
            * temperature_effect
            * (1.0 - wetness_reduction)
            * roughness_effect;
        SurfaceGripModifier {
            temperature_effect,
            wetness_reduction,
            roughness_effect,
            curb_bump,
            total_grip_modifier,
        }
    }

    /// Riduzione di grip per bagnato/acqua (port di `calculateWetGripReduction`).
    pub fn calculate_wet_grip_reduction(&self, wetness: f32, water_depth: f32) -> f32 {
        let mut reduction = wetness * 0.3;
        if water_depth > 0.001 {
            let hydroplaning_risk = (water_depth / 0.005).min(1.0);
            reduction += hydroplaning_risk * 0.4;
        }
        reduction.clamp(0.0, 0.8)
    }

    /// Forza dell'urto sul cordolo (port di `calculateCurbEffect`).
    pub fn calculate_curb_effect(&self, curb_height: f32, suspension_compression: f32) -> f32 {
        if curb_height < 0.01 {
            return 0.0;
        }
        let compression_ratio = (suspension_compression / 0.1).clamp(0.0, 1.0);
        curb_height * 5000.0 * (1.0 - compression_ratio)
    }

    /// Resistenza al rotolamento (port di `calculateRollingResistance`).
    pub fn calculate_rolling_resistance(&self, surface: &SurfaceProperties, speed: f32) -> f32 {
        let base = surface.rolling_resistance;
        let speed_factor = 1.0 + speed * 0.001;
        let roughness_factor = 1.0 + surface.surface_roughness * 0.5;
        base * speed_factor * roughness_factor
    }
}

// =============================================================================
// BANKING / ELEVAZIONE
// =============================================================================

/// Stato di banking (port di `BankingState`).
#[derive(Clone, Copy, Debug, Default)]
pub struct BankingState {
    pub banking_angle: f32,
    pub normal_force_gain: f32,
    pub cornering_enhancement: f32,
    pub lateral_accel_contribution: f32,
}

/// Modello di banking (port di `BankingModel`).
#[derive(Clone, Copy, Debug, Default)]
pub struct BankingModel;

const GRAVITY: f32 = 9.81;

impl BankingModel {
    pub fn calculate(
        &self,
        banking_angle: f32,
        speed: f32,
        mass: f32,
        corner_radius: f32,
    ) -> BankingState {
        let sin_angle = banking_angle.sin();
        let cos_angle = banking_angle.cos();
        let mut state = BankingState {
            banking_angle,
            normal_force_gain: mass * GRAVITY * sin_angle,
            cornering_enhancement: if cos_angle > 0.0 {
                1.0 / cos_angle
            } else {
                1.0
            },
            ..Default::default()
        };
        if corner_radius > 0.0 {
            let centrifugal_force = mass * speed * speed / corner_radius;
            state.lateral_accel_contribution = centrifugal_force * sin_angle;
        }
        state
    }

    pub fn normal_force_contribution(
        &self,
        banking_angle: f32,
        mass: f32,
        lateral_accel: f32,
    ) -> f32 {
        let sin_angle = banking_angle.sin();
        let cos_angle = banking_angle.cos();
        mass * GRAVITY * cos_angle + mass * lateral_accel * sin_angle
    }

    pub fn cornering_enhancement(&self, banking_angle: f32, speed: f32, corner_radius: f32) -> f32 {
        let cos_angle = banking_angle.cos();
        if cos_angle <= 0.0 {
            return 1.0;
        }
        let optimal_speed = (GRAVITY * corner_radius * banking_angle.tan()).sqrt();
        let speed_ratio = if optimal_speed > 0.0 {
            (speed / optimal_speed).clamp(0.0, 2.0)
        } else {
            0.0
        };
        1.0 + speed_ratio * (1.0 / cos_angle - 1.0)
    }

    pub fn speed_limit(&self, banking_angle: f32, corner_radius: f32, max_grip: f32) -> f32 {
        let sin_angle = banking_angle.sin();
        let cos_angle = banking_angle.cos();
        let max_lateral_accel = max_grip * GRAVITY;
        let banking_contribution = if cos_angle != 0.0 {
            GRAVITY * sin_angle / cos_angle
        } else {
            0.0
        };
        let available = max_lateral_accel + banking_contribution;
        if available <= 0.0 {
            return 0.0;
        }
        (available * corner_radius).sqrt()
    }
}

/// Stato di elevazione (port di `ElevationState`).
#[derive(Clone, Copy, Debug, Default)]
pub struct ElevationState {
    pub gradient: f32,
    pub vertical_acceleration: f32,
    pub load_variation: f32,
    pub ground_effect_modifier: f32,
}

/// Modello di elevazione (port di `ElevationModel`).
#[derive(Clone, Copy, Debug, Default)]
pub struct ElevationModel;

impl ElevationModel {
    pub fn calculate(
        &self,
        track_gradient: f32,
        speed: f32,
        _lateral_accel: f32,
        ride_height_front: f32,
        ride_height_rear: f32,
    ) -> ElevationState {
        let mass = 1500.0;
        let avg_ride_height = (ride_height_front + ride_height_rear) * 0.5;
        ElevationState {
            gradient: track_gradient,
            vertical_acceleration: self.vertical_acceleration(track_gradient, speed),
            load_variation: self.load_variation(track_gradient, mass, 0.0),
            ground_effect_modifier: self.ground_effect_modifier(avg_ride_height, speed, 1.0),
        }
    }

    pub fn vertical_acceleration(&self, gradient_change: f32, _speed: f32) -> f32 {
        GRAVITY * gradient_change.sin()
    }

    pub fn load_variation(&self, gradient: f32, mass: f32, longitudinal_accel: f32) -> f32 {
        let gradient_effect = mass * GRAVITY * gradient.sin();
        let accel_effect = mass * longitudinal_accel * gradient.cos();
        gradient_effect + accel_effect
    }

    pub fn ground_effect_modifier(
        &self,
        ride_height: f32,
        speed: f32,
        ground_effect_factor: f32,
    ) -> f32 {
        let reference_ride_height = 0.05;
        if ride_height <= 0.0 {
            return 1.0;
        }
        let height_ratio = (reference_ride_height / ride_height).clamp(0.5, 2.0);
        let speed_factor = if speed > 10.0 {
            1.0 + (speed - 10.0) * 0.001
        } else {
            1.0
        };
        (height_ratio * speed_factor * ground_effect_factor).clamp(0.3, 2.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_grid_samples_clean_track() {
        let grid = TrackSurface::new();
        let s = grid.sample(Vec3::ZERO);
        assert!((s.grip - 1.0).abs() < 1e-6);
        assert_eq!(s.wetness, 0.0);
        assert_eq!(s.marbles, 0.0);
    }

    #[test]
    fn outside_world_uses_off_track_grip() {
        let grid = TrackSurface::new();
        let s = grid.sample(Vec3::new(5000.0, 0.0, 0.0));
        assert!((s.grip - grid.off_track_grip()).abs() < 1e-6);
    }

    #[test]
    fn wetness_reduces_grip() {
        let mut grid = TrackSurface::new();
        grid.set_wetness(1.0);
        let s = grid.sample(Vec3::ZERO);
        assert!((s.grip - (1.0 - 0.45)).abs() < 1e-6, "got {}", s.grip);
    }

    #[test]
    fn rubber_raises_grip_and_marbles_lower_it() {
        let mut grid = TrackSurface::new();
        let p = Vec3::new(10.0, 0.0, 10.0);
        grid.deposit_rubber(p, 1.0, 3.0);
        let rubbered = grid.sample(p);
        assert!(rubbered.grip > 1.0, "rubber raises grip");
        assert!(rubbered.rubber > 0.0);

        let mut grid = TrackSurface::new();
        grid.add_marbles(p, 1.0, 3.0);
        let marbled = grid.sample(p);
        assert!(marbled.grip < 1.0, "marbles lower grip");
    }

    #[test]
    fn deposit_weight_is_one_at_cell_center() {
        let mut grid = TrackSurface::new();
        let p = Vec3::new(10.0, 0.0, 10.0);
        grid.deposit_rubber(p, 0.5, 5.0);
        let center = grid.sample(p);
        assert!((center.rubber - 0.5).abs() < 0.05, "got {}", center.rubber);
    }

    #[test]
    fn material_grip_is_case_insensitive() {
        let mut grid = TrackSurface::new();
        grid.set_material_grip("kerb", 0.8);
        assert!((grid.material_grip("KERB") - 0.8).abs() < 1e-6);
        assert!((grid.material_grip("road") - grid.grip(Vec3::ZERO)).abs() < 1e-6);
    }

    #[test]
    fn sync_from_weather_sets_grid_wide() {
        let mut grid = TrackSurface::new();
        grid.sync_from_weather(0.5, 40.0);
        let s = grid.sample(Vec3::ZERO);
        assert_eq!(s.wetness, 0.5);
        assert_eq!(s.temperature, 40.0);
    }

    #[test]
    fn configure_clamps_resolution() {
        let mut grid = TrackSurface::new();
        grid.configure(10, 1000.0);
        // 10 clamps to 32; a sample inside the world still resolves.
        let s = grid.sample(Vec3::ZERO);
        assert!(s.grip > 0.0);
    }

    #[test]
    fn surface_model_sections() {
        let mut model = TrackSurfaceModel::new();
        model.add_surface(
            0.0,
            100.0,
            SurfaceProperties {
                grip_coefficient: 0.6,
                is_curb: true,
                ..Default::default()
            },
        );
        assert_eq!(model.surface_at(50.0).grip_coefficient, 0.6);
        assert!(model.surface_at(50.0).is_curb);
        assert_eq!(model.surface_at(150.0).grip_coefficient, 1.0, "default");
    }

    #[test]
    fn grip_modifier_math() {
        let model = TrackSurfaceModel::new();
        let surface = SurfaceProperties::default();
        let m = model.calculate_grip_modifier(&surface, 80.0, 25.0, 30.0);
        assert!(
            (m.temperature_effect - 1.0).abs() < 1e-6,
            "optimal tire temp"
        );
        assert_eq!(m.total_grip_modifier, 1.0);

        // Cold tires lose grip (clamped to 0.5 at the extreme).
        let cold = model.calculate_grip_modifier(&surface, 0.0, 25.0, 30.0);
        assert!(cold.temperature_effect < 1.0);
        assert!(cold.temperature_effect >= 0.5);
    }

    #[test]
    fn wet_grip_reduction_grows_with_water() {
        let model = TrackSurfaceModel::new();
        let dry = model.calculate_wet_grip_reduction(0.0, 0.0);
        let wet = model.calculate_wet_grip_reduction(1.0, 0.0);
        let flooded = model.calculate_wet_grip_reduction(1.0, 0.01);
        assert_eq!(dry, 0.0);
        assert!(wet > dry);
        assert!(flooded > wet);
        assert!(flooded <= 0.8);
    }

    #[test]
    fn curb_effect_needs_height() {
        let model = TrackSurfaceModel::new();
        assert_eq!(model.calculate_curb_effect(0.0, 0.0), 0.0);
        let uncompressed = model.calculate_curb_effect(0.05, 0.0);
        let compressed = model.calculate_curb_effect(0.05, 0.1);
        assert!(uncompressed > compressed, "compression absorbs the bump");
        assert_eq!(compressed, 0.0);
    }

    #[test]
    fn rolling_resistance_scales_with_speed_and_roughness() {
        let model = TrackSurfaceModel::new();
        let base = SurfaceProperties::default();
        let slow = model.calculate_rolling_resistance(&base, 0.0);
        let fast = model.calculate_rolling_resistance(&base, 100.0);
        assert!(fast > slow);
        let rough = SurfaceProperties {
            surface_roughness: 1.0,
            ..base
        };
        assert!(model.calculate_rolling_resistance(&rough, 0.0) > slow);
    }

    #[test]
    fn banking_models() {
        let model = BankingModel;
        let state = model.calculate(0.1, 40.0, 1200.0, 100.0);
        assert!((state.banking_angle - 0.1).abs() < 1e-6);
        assert!(state.normal_force_gain > 0.0);
        assert!(state.cornering_enhancement > 1.0);
        assert!(state.lateral_accel_contribution > 0.0);

        let flat = model.calculate(0.0, 40.0, 1200.0, 0.0);
        assert_eq!(flat.normal_force_gain, 0.0);
        assert_eq!(flat.cornering_enhancement, 1.0);

        let limit = model.speed_limit(0.2, 80.0, 1.2);
        assert!(limit > 0.0);
    }

    #[test]
    fn elevation_models() {
        let model = ElevationModel;
        let state = model.calculate(0.1, 30.0, 0.0, 0.07, 0.09);
        assert_eq!(state.gradient, 0.1);
        assert!(state.vertical_acceleration > 0.0);
        assert!(state.ground_effect_modifier > 0.0);

        assert_eq!(model.ground_effect_modifier(0.0, 50.0, 1.0), 1.0);
        let low = model.ground_effect_modifier(0.02, 50.0, 1.0);
        let high = model.ground_effect_modifier(0.2, 50.0, 1.0);
        assert!(low > high, "lower ride height grips more");
    }
}
