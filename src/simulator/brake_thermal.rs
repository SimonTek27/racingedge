//! Brake — disc/piston thermal model and pad/disc wear.
//!
//! Port of `src/engine/physics/BrakeThermalModel.{h,cpp}` from ksengine, plus
//! the wear system `BrakeWearSystem.{h,cpp}`. The latter is *dead code*
//! in the reference (excluded from source list because it redeclares
//! `BrakeThermalConfig`/`BrakeThermalState` with a different layout): here
//! the wear system's thermal types are renamed `BrakeWearThermal*` to avoid
//! collision, as suggested by the note in `BrakeWearSystem.h`.

// ── Modello termico per angolo (BrakeThermalModel) ──────────────────────────

/// Configurazione del modello termico per angolo (port di `BrakeThermalConfig`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrakeThermalConfig {
    /// Massa del disco, in kg.
    pub disc_mass: f32,
    /// Calore specifico del disco, J/(kg·K).
    pub disc_specific_heat: f32,
    /// Coefficiente di raffreddamento, W/K a 1 m/s.
    pub cooling_coeff: f32,
    /// Temperatura ambiente, °C.
    pub ambient_temp: f32,
    /// Temperatura di inizio fade, °C.
    pub fade_start_temp: f32,
    /// Temperatura di fade completo, °C.
    pub fade_full_temp: f32,
    /// Temperatura massima, °C.
    pub max_temp: f32,
}

impl Default for BrakeThermalConfig {
    fn default() -> Self {
        Self {
            disc_mass: 6.5,
            disc_specific_heat: 480.0,
            cooling_coeff: 12.0,
            ambient_temp: 25.0,
            fade_start_temp: 320.0,
            fade_full_temp: 620.0,
            max_temp: 900.0,
        }
    }
}

/// Stato termico di un angolo (port di `BrakeThermalState`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrakeThermalState {
    /// Temperatura del disco, °C.
    pub disc_temp: f32,
    /// Temperatura della pinza, °C.
    pub pad_temp: f32,
    /// Riduzione di coppia da fade, `0..=1`.
    pub fade: f32,
    /// Energia accumulata in questo passo, J.
    pub energy_accum: f32,
}

impl Default for BrakeThermalState {
    fn default() -> Self {
        Self {
            disc_temp: 80.0,
            pad_temp: 60.0,
            fade: 0.0,
            energy_accum: 0.0,
        }
    }
}

/// Modello termico freno per i quattro angoli (port di `BrakeThermalModel`).
#[derive(Clone, Debug, Default)]
pub struct BrakeThermalModel {
    config: BrakeThermalConfig,
    state: [BrakeThermalState; 4],
}

impl BrakeThermalModel {
    /// Un modello termico con la configurazione predefinita.
    pub fn new() -> Self {
        Self::default()
    }

    /// Imposta la configurazione.
    pub fn set_config(&mut self, config: BrakeThermalConfig) {
        self.config = config;
    }

    /// La configurazione corrente.
    pub fn config(&self) -> BrakeThermalConfig {
        self.config
    }

    /// Un passo di integrazione per l'angolo `wheel` (port di `update`).
    ///
    /// * `brake_torque_nm` — coppia frenante applicata alla ruota.
    /// * `wheel_omega` — velocità angolare della ruota, rad/s.
    /// * `speed_ms` — velocità del veicolo (flusso d'aria di raffreddamento).
    pub fn update(
        &mut self,
        dt: f32,
        wheel: usize,
        brake_torque_nm: f32,
        wheel_omega: f32,
        speed_ms: f32,
    ) {
        if wheel >= 4 {
            return;
        }
        let dt = dt.clamp(1e-4, 0.05);

        let power = (brake_torque_nm * wheel_omega).abs();
        let heat_capacity = (self.config.disc_mass * self.config.disc_specific_heat).max(1.0);
        let d_heat = power * dt / heat_capacity;

        let cool_w_per_k = self.config.cooling_coeff * (1.0 + 0.05 * speed_ms.max(0.0));
        let d_cool = -cool_w_per_k * (self.state[wheel].disc_temp - self.config.ambient_temp) * dt
            / heat_capacity;

        let s = &mut self.state[wheel];
        s.energy_accum = power * dt;
        s.disc_temp =
            (s.disc_temp + d_heat + d_cool).clamp(self.config.ambient_temp, self.config.max_temp);
        s.pad_temp += (s.disc_temp - s.pad_temp) * 0.15 * dt;

        s.fade = if s.disc_temp <= self.config.fade_start_temp {
            0.0
        } else if s.disc_temp >= self.config.fade_full_temp {
            1.0
        } else {
            (s.disc_temp - self.config.fade_start_temp)
                / (self.config.fade_full_temp - self.config.fade_start_temp)
        };
    }

    /// Lo stato termico dell'angolo `wheel` (stato nullo se fuori range).
    pub fn state(&self, wheel: usize) -> BrakeThermalState {
        if wheel < 4 {
            self.state[wheel]
        } else {
            BrakeThermalState::default()
        }
    }

    /// Il fade dell'angolo `wheel`.
    pub fn fade(&self, wheel: usize) -> f32 {
        self.state(wheel).fade
    }

    /// La temperatura del disco dell'angolo `wheel`.
    pub fn disc_temp(&self, wheel: usize) -> f32 {
        self.state(wheel).disc_temp
    }

    /// La coppia effettiva dopo il fade (port di `effectiveTorque`).
    pub fn effective_torque(&self, wheel: usize, commanded_torque: f32) -> f32 {
        commanded_torque * (1.0 - self.fade(wheel))
    }

    /// Riporta tutti gli angoli allo stato iniziale (port di `reset`).
    pub fn reset(&mut self) {
        self.state = [BrakeThermalState::default(); 4];
    }
}

// ── Sistema di usura (BrakeWearSystem) ──────────────────────────────────────

/// Configurazione termica del sistema di usura (port di `BrakeThermalConfig`
/// di `BrakeWearSystem.h`, qui rinominata `BrakeWearThermalConfig`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrakeWearThermalConfig {
    /// Massa del disco, in kg.
    pub disc_mass: f32,
    /// Massa della pastiglia, in kg.
    pub pad_mass: f32,
    /// Calore specifico del disco, J/(kg·K).
    pub disc_heat_capacity: f32,
    /// Calore specifico della pastiglia, J/(kg·K).
    pub pad_heat_capacity: f32,
    /// Coefficiente di attrito base.
    pub friction_coeff_base: f32,
    /// Sensibilità dell'attrito alla temperatura.
    pub friction_temp_sensitivity: f32,
    /// Raffreddamento anteriore, W/(m²·K).
    pub cooling_coeff_front: f32,
    /// Raffreddamento posteriore, W/(m²·K).
    pub cooling_coeff_rear: f32,
    /// Area della superficie del disco, m².
    pub disc_area: f32,
    /// Conduzione pinza→disco.
    pub conduction_pad_to_disc: f32,
}

impl Default for BrakeWearThermalConfig {
    fn default() -> Self {
        Self {
            disc_mass: 4.0,
            pad_mass: 0.5,
            disc_heat_capacity: 500.0,
            pad_heat_capacity: 900.0,
            friction_coeff_base: 0.42,
            friction_temp_sensitivity: 0.0005,
            cooling_coeff_front: 45.0,
            cooling_coeff_rear: 35.0,
            disc_area: 0.03,
            conduction_pad_to_disc: 0.6,
        }
    }
}

/// Stato termico del sistema di usura (port di `BrakeThermalState` di
/// `BrakeWearSystem.h`, rinominata `BrakeWearThermalState`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrakeWearThermalState {
    /// Temperatura del disco per angolo, °C.
    pub disc_temp: [f32; 4],
    /// Temperatura della pastiglia per angolo, °C.
    pub pad_temp: [f32; 4],
    /// Temperatura del mozzo per angolo, °C.
    pub hub_temp: [f32; 4],
    /// Temperatura ambiente, °C.
    pub ambient_temp: f32,
    /// Velocità del flusso d'aria, m/s.
    pub airflow_speed: f32,
    /// Minimo della finestra ottimale del disco, °C.
    pub optimal_disc_temp_min: f32,
    /// Massimo della finestra ottimale del disco, °C.
    pub optimal_disc_temp_max: f32,
    /// Temperatura critica del disco, °C.
    pub critical_disc_temp: f32,
    /// Temperatura di inizio fade, °C.
    pub fade_onset_temp: f32,
}

impl Default for BrakeWearThermalState {
    fn default() -> Self {
        Self {
            disc_temp: [300.0; 4],
            pad_temp: [250.0; 4],
            hub_temp: [80.0; 4],
            ambient_temp: 25.0,
            airflow_speed: 0.0,
            optimal_disc_temp_min: 250.0,
            optimal_disc_temp_max: 500.0,
            critical_disc_temp: 800.0,
            fade_onset_temp: 600.0,
        }
    }
}

/// Tipo di fade (port di `BrakeFadeType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BrakeFadeType {
    /// Nessun fade.
    #[default]
    None,
    /// Fade termico da surriscaldamento.
    Thermal,
    /// Degrado del materiale della pastiglia.
    Friction,
    /// Outgassing ad alta temperatura.
    Gas,
    /// Deformazione fisica.
    Mechanical,
}

/// Stato di fade per angolo (port di `BrakeFadeState`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrakeFadeState {
    /// Tipo di fade corrente.
    pub fade_type: BrakeFadeType,
    /// Livello di fade, `0..=1`.
    pub fade_level: f32,
    /// Attrito di picco corrente.
    pub peak_friction: f32,
    /// Durezza del pedale (`1` = duro, `0` = spugnoso).
    pub pedal_firmness: f32,
    /// Ritardo di risposta aggiunto, s.
    pub response_time: f32,
    /// Velocità di recupero del fade.
    pub recovery_rate: f32,
}

impl Default for BrakeFadeState {
    fn default() -> Self {
        Self {
            fade_type: BrakeFadeType::None,
            fade_level: 0.0,
            peak_friction: 0.42,
            pedal_firmness: 1.0,
            response_time: 0.0,
            recovery_rate: 0.1,
        }
    }
}

impl BrakeFadeState {
    /// Attrito effettivo dopo il fade (port di `effectiveFriction`).
    pub fn effective_friction(&self, base_friction: f32) -> f32 {
        base_friction * (1.0 - self.fade_level * 0.6)
    }

    /// Moltiplicatore della forza frenante (port di `brakingMultiplier`).
    pub fn braking_multiplier(&self) -> f32 {
        1.0 - self.fade_level * 0.5
    }
}

/// Stato di usura delle pastiglie (port di `BrakePadWearState`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrakePadWearState {
    /// Spessore residuo delle pastiglie per angolo, mm.
    pub pad_thickness: [f32; 4],
    /// Spessore iniziale, mm.
    pub initial_thickness: f32,
    /// Velocità di usura per angolo, mm/s.
    pub wear_rate: [f32; 4],
    /// Usura totale per angolo, mm.
    pub total_wear: [f32; 4],
    /// Se la pastiglia va sostituita, per angolo.
    pub needs_replacement: [bool; 4],
}

impl Default for BrakePadWearState {
    fn default() -> Self {
        Self {
            pad_thickness: [10.0; 4],
            initial_thickness: 10.0,
            wear_rate: [0.0; 4],
            total_wear: [0.0; 4],
            needs_replacement: [false; 4],
        }
    }
}

impl BrakePadWearState {
    /// Vita residua frazionaria dell'angolo (port di `remainingLife`).
    pub fn remaining_life(&self, wheel: usize) -> f32 {
        if wheel >= 4 {
            return 0.0;
        }
        (self.pad_thickness[wheel] / self.initial_thickness).clamp(0.0, 1.0)
    }

    /// Se la pastiglia è consumata (port di `isWornOut`).
    pub fn is_worn_out(&self, wheel: usize) -> bool {
        wheel < 4 && self.pad_thickness[wheel] < 1.0
    }
}

/// Configurazione dell'usura delle pastiglie (port di `BrakePadWearConfig`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrakePadWearConfig {
    /// Usura base, mm/s a 1000 Nm e 400 °C.
    pub base_wear_rate: f32,
    /// Esponente della pressione.
    pub pressure_exponent: f32,
    /// Esponente della temperatura.
    pub temperature_exponent: f32,
    /// Esponente della velocità.
    pub velocity_exponent: f32,
    /// Fattore del materiale (`1.0` = standard).
    pub pad_material_factor: f32,
    /// Spessore minimo, mm.
    pub minimum_thickness: f32,
    /// Spessore di avviso, mm.
    pub warning_thickness: f32,
}

impl Default for BrakePadWearConfig {
    fn default() -> Self {
        Self {
            base_wear_rate: 0.001,
            pressure_exponent: 1.1,
            temperature_exponent: 1.3,
            velocity_exponent: 0.9,
            pad_material_factor: 1.0,
            minimum_thickness: 1.0,
            warning_thickness: 3.0,
        }
    }
}

/// Stato di usura dei dischi (port di `BrakeDiscWearState`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrakeDiscWearState {
    /// Spessore residuo dei dischi per angolo, mm.
    pub disc_thickness: [f32; 4],
    /// Spessore iniziale, mm.
    pub initial_thickness: f32,
    /// Rugosità superficiale per angolo (`0` = liscia).
    pub surface_roughness: [f32; 4],
    /// Deformazione del disco per angolo, `0..=1`.
    pub warpage: [f32; 4],
    /// Se il disco è deformato, per angolo.
    pub is_warped: [bool; 4],
}

impl Default for BrakeDiscWearState {
    fn default() -> Self {
        Self {
            disc_thickness: [28.0; 4],
            initial_thickness: 28.0,
            surface_roughness: [0.0; 4],
            warpage: [0.0; 4],
            is_warped: [false; 4],
        }
    }
}

impl BrakeDiscWearState {
    /// Vita residua frazionaria del disco (port di `remainingLife`).
    pub fn remaining_life(&self, wheel: usize) -> f32 {
        if wheel >= 4 {
            return 0.0;
        }
        (self.disc_thickness[wheel] / self.initial_thickness).clamp(0.0, 1.0)
    }
}

/// Dati di ingresso per un passo di usura (port di `BrakeWearData`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrakeWearData {
    /// Angolo (0..=3).
    pub wheel: usize,
    /// Coppia frenante applicata, Nm.
    pub brake_torque: f32,
    /// Pressione idraulica, bar.
    pub brake_pressure: f32,
    /// Velocità angolare della ruota, rad/s.
    pub wheel_speed: f32,
    /// Velocità del veicolo, m/s.
    pub vehicle_speed: f32,
    /// Carico verticale sullo pneumatico, N.
    pub normal_load: f32,
    /// Passo di tempo, s.
    pub dt: f32,
}

impl Default for BrakeWearData {
    fn default() -> Self {
        Self {
            wheel: 0,
            brake_torque: 0.0,
            brake_pressure: 0.0,
            wheel_speed: 0.0,
            vehicle_speed: 0.0,
            normal_load: 0.0,
            dt: 0.016,
        }
    }
}

/// Sistema di usura freni (port di `BrakeWearSystem`).
#[derive(Clone, Debug)]
pub struct BrakeWearSystem {
    thermal_config: BrakeWearThermalConfig,
    pad_wear_config: BrakePadWearConfig,
    thermal: BrakeWearThermalState,
    fade: [BrakeFadeState; 4],
    pad_wear: BrakePadWearState,
    disc_wear: BrakeDiscWearState,
    braking: [bool; 4],
}

impl Default for BrakeWearSystem {
    fn default() -> Self {
        let mut system = Self {
            thermal_config: BrakeWearThermalConfig::default(),
            pad_wear_config: BrakePadWearConfig::default(),
            thermal: BrakeWearThermalState::default(),
            fade: [BrakeFadeState::default(); 4],
            pad_wear: BrakePadWearState::default(),
            disc_wear: BrakeDiscWearState::default(),
            braking: [false; 4],
        };
        system.reset();
        system
    }
}

impl BrakeWearSystem {
    /// Un sistema di usura nello stato iniziale.
    pub fn new() -> Self {
        Self::default()
    }

    /// Imposta la configurazione termica.
    pub fn set_thermal_config(&mut self, config: BrakeWearThermalConfig) {
        self.thermal_config = config;
    }

    /// Imposta la configurazione di usura pastiglie.
    pub fn set_pad_wear_config(&mut self, config: BrakePadWearConfig) {
        self.pad_wear_config = config;
    }

    /// Imposta la temperatura ambiente.
    pub fn set_ambient_temp(&mut self, temp: f32) {
        self.thermal.ambient_temp = temp;
    }

    /// Un passo di usura (port di `update`).
    pub fn update(&mut self, data: BrakeWearData) {
        let w = data.wheel.min(3);
        self.thermal.airflow_speed = data.vehicle_speed;
        if data.brake_pressure > 0.5 || data.brake_torque > 10.0 {
            self.braking[w] = true;
            self.update_thermal(w, data.brake_torque, data.wheel_speed, data.dt);
            self.update_fade(w, data.dt);
            self.update_pad_wear(w, data.brake_torque, data.wheel_speed, data.dt);
            self.update_disc_wear(w, data.brake_torque, data.wheel_speed, data.dt);
        } else {
            self.braking[w] = false;
            self.update_thermal(w, 0.0, data.wheel_speed, data.dt);
            self.update_fade(w, data.dt);
        }
    }

    /// Applica una pressione di frenata all'angolo (port di `applyBrake`).
    pub fn apply_brake(&mut self, wheel: usize, pressure: f32, dt: f32) {
        let wheel = wheel.min(3);
        let torque = self.calculate_brake_torque(pressure, wheel);
        let data = BrakeWearData {
            wheel,
            brake_pressure: pressure,
            brake_torque: torque,
            dt,
            vehicle_speed: self.thermal.airflow_speed,
            ..Default::default()
        };
        self.update(data);
    }

    /// Lo stato termico corrente.
    pub fn thermal_state(&self) -> BrakeWearThermalState {
        self.thermal
    }

    /// Lo stato di fade dell'angolo.
    pub fn fade_state(&self, wheel: usize) -> BrakeFadeState {
        if wheel < 4 {
            self.fade[wheel]
        } else {
            BrakeFadeState::default()
        }
    }

    /// Lo stato di usura delle pastiglie.
    pub fn pad_wear_state(&self) -> BrakePadWearState {
        self.pad_wear
    }

    /// Lo stato di usura dei dischi.
    pub fn disc_wear_state(&self) -> BrakeDiscWearState {
        self.disc_wear
    }

    /// Temperatura del disco dell'angolo.
    pub fn disc_temperature(&self, wheel: usize) -> f32 {
        if wheel < 4 {
            self.thermal.disc_temp[wheel]
        } else {
            0.0
        }
    }

    /// Temperatura della pastiglia dell'angolo.
    pub fn pad_temperature(&self, wheel: usize) -> f32 {
        if wheel < 4 {
            self.thermal.pad_temp[wheel]
        } else {
            0.0
        }
    }

    /// Attrito effettivo dell'angolo (port di `effectiveFriction`).
    pub fn effective_friction(&self, wheel: usize) -> f32 {
        let wheel = wheel.min(3);
        let mut base = self.thermal_config.friction_coeff_base;
        base +=
            (self.thermal.disc_temp[wheel] - 400.0) * self.thermal_config.friction_temp_sensitivity;
        self.fade[wheel].effective_friction(base.max(0.05))
    }

    /// Moltiplicatore della frenata dell'angolo (port di `brakingMultiplier`).
    pub fn braking_multiplier(&self, wheel: usize) -> f32 {
        let wheel = wheel.min(3);
        let fade_effect = self.fade[wheel].braking_multiplier();
        let pad_effect = self.pad_wear.remaining_life(wheel);
        let disc_effect = self.disc_wear.remaining_life(wheel);
        (fade_effect * pad_effect * disc_effect).clamp(0.1, 1.0)
    }

    /// Se l'angolo è in fade (port di `isFaded`).
    pub fn is_faded(&self, wheel: usize) -> bool {
        wheel < 4 && self.fade[wheel].fade_level > 0.1
    }

    /// Se la pastiglia dell'angolo va sostituita.
    pub fn needs_pad_replacement(&self, wheel: usize) -> bool {
        self.pad_wear.is_worn_out(wheel)
    }

    /// Se il disco dell'angolo va sostituito.
    pub fn needs_disc_replacement(&self, wheel: usize) -> bool {
        wheel < 4 && self.disc_wear.disc_thickness[wheel] < 15.0
    }

    /// Riporta il sistema allo stato iniziale (port di `reset`).
    pub fn reset(&mut self) {
        self.thermal = BrakeWearThermalState::default();
        self.fade = [BrakeFadeState::default(); 4];
        self.braking = [false; 4];
        self.pad_wear = BrakePadWearState::default();
        self.disc_wear = BrakeDiscWearState::default();
    }

    fn update_thermal(&mut self, wheel: usize, brake_torque: f32, wheel_speed: f32, dt: f32) {
        let heat = (brake_torque * wheel_speed).abs();
        let cool = self.calculate_cooling(wheel, self.thermal.airflow_speed);
        let disc_c = self.thermal_config.disc_mass * self.thermal_config.disc_heat_capacity;
        let d_temp = (heat - cool) * dt / disc_c.max(1.0);
        self.thermal.disc_temp[wheel] =
            (self.thermal.disc_temp[wheel] + d_temp).clamp(self.thermal.ambient_temp, 1200.0);

        let pad_share = heat * self.thermal_config.conduction_pad_to_disc;
        let pad_c = self.thermal_config.pad_mass * self.thermal_config.pad_heat_capacity;
        self.thermal.pad_temp[wheel] = (self.thermal.pad_temp[wheel]
            + pad_share * dt / pad_c.max(1.0)
            - cool * 0.2 * dt / pad_c.max(1.0))
        .clamp(self.thermal.ambient_temp, 1000.0);
    }

    fn update_fade(&mut self, wheel: usize, dt: f32) {
        let temp = self.thermal.disc_temp[wheel];
        let f = &mut self.fade[wheel];
        if temp > self.thermal.fade_onset_temp {
            let over = (temp - self.thermal.fade_onset_temp)
                / (self.thermal.critical_disc_temp - self.thermal.fade_onset_temp).max(1.0);
            f.fade_level = (f.fade_level + over * dt * 0.5).clamp(0.0, 1.0);
            f.fade_type = BrakeFadeType::Thermal;
            f.pedal_firmness = 1.0 - f.fade_level * 0.6;
        } else {
            f.fade_level = (f.fade_level - f.recovery_rate * dt).max(0.0);
            if f.fade_level < 0.01 {
                f.fade_level = 0.0;
                f.fade_type = BrakeFadeType::None;
                f.pedal_firmness = 1.0;
            }
        }
        f.peak_friction = self.thermal_config.friction_coeff_base * (1.0 - f.fade_level * 0.8);
    }

    fn update_pad_wear(&mut self, wheel: usize, brake_torque: f32, wheel_speed: f32, dt: f32) {
        let temp_factor = (self.thermal.pad_temp[wheel].max(1.0) / 400.0)
            .powf(self.pad_wear_config.temperature_exponent);
        let press_factor =
            ((brake_torque.abs() / 1000.0).max(0.01)).powf(self.pad_wear_config.pressure_exponent);
        let vel_factor =
            (wheel_speed.abs().max(0.01) / 50.0).powf(self.pad_wear_config.velocity_exponent);
        let rate = self.pad_wear_config.base_wear_rate * temp_factor * press_factor * vel_factor;
        self.pad_wear.wear_rate[wheel] = rate;
        self.pad_wear.pad_thickness[wheel] =
            (self.pad_wear.pad_thickness[wheel] - rate * dt).max(0.0);
        self.pad_wear.total_wear[wheel] += rate * dt;
        self.pad_wear.needs_replacement[wheel] =
            self.pad_wear.pad_thickness[wheel] < self.pad_wear_config.warning_thickness;
    }

    fn update_disc_wear(&mut self, wheel: usize, brake_torque: f32, wheel_speed: f32, dt: f32) {
        let rate = self.pad_wear_config.base_wear_rate
            * 0.05
            * (brake_torque.abs() / 1000.0)
            * (wheel_speed.abs() / 50.0);
        self.disc_wear.disc_thickness[wheel] =
            (self.disc_wear.disc_thickness[wheel] - rate * dt).max(5.0);
        if self.thermal.disc_temp[wheel] > 700.0 {
            self.disc_wear.warpage[wheel] =
                (self.disc_wear.warpage[wheel] + 0.01 * dt).clamp(0.0, 1.0);
        }
        self.disc_wear.is_warped[wheel] = self.disc_wear.warpage[wheel] > 0.3;
    }

    fn calculate_brake_torque(&self, pressure: f32, wheel: usize) -> f32 {
        pressure * 80.0 * self.effective_friction(wheel)
    }

    fn calculate_cooling(&self, wheel: usize, speed: f32) -> f32 {
        let coeff = if wheel < 2 {
            self.thermal_config.cooling_coeff_front
        } else {
            self.thermal_config.cooling_coeff_rear
        };
        let d_temp = self.thermal.disc_temp[wheel] - self.thermal.ambient_temp;
        let air = 1.0 + 0.05 * speed.max(0.0);
        coeff * self.thermal_config.disc_area * d_temp * air
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disc_heats_under_braking() {
        let mut model = BrakeThermalModel::new();
        let before = model.disc_temp(0);
        for _ in 0..200 {
            model.update(0.016, 0, 2000.0, 100.0, 0.0);
        }
        assert!(model.disc_temp(0) > before);
    }

    #[test]
    fn disc_cools_when_idle() {
        let mut model = BrakeThermalModel::new();
        for _ in 0..500 {
            model.update(0.016, 0, 3000.0, 100.0, 0.0);
        }
        let hot = model.disc_temp(0);
        for _ in 0..2000 {
            model.update(0.016, 0, 0.0, 0.0, 30.0);
        }
        assert!(model.disc_temp(0) < hot);
    }

    #[test]
    fn fade_appears_and_reduces_effective_torque() {
        let mut model = BrakeThermalModel::new();
        for _ in 0..5000 {
            model.update(0.016, 0, 4000.0, 150.0, 0.0);
        }
        assert!(model.fade(0) > 0.0);
        assert!(model.effective_torque(0, 1000.0) < 1000.0);
    }

    #[test]
    fn out_of_range_wheel_is_safe() {
        let mut model = BrakeThermalModel::new();
        model.update(0.016, 9, 1000.0, 100.0, 0.0);
        assert_eq!(model.state(9), BrakeThermalState::default());
    }

    #[test]
    fn wear_system_cools_and_heats() {
        let mut sys = BrakeWearSystem::new();
        let cold = sys.disc_temperature(0);
        sys.update(BrakeWearData {
            wheel: 0,
            brake_torque: 2500.0,
            brake_pressure: 60.0,
            wheel_speed: 120.0,
            vehicle_speed: 40.0,
            ..Default::default()
        });
        assert!(sys.disc_temperature(0) > cold);
    }

    #[test]
    fn pad_wears_under_load() {
        let mut sys = BrakeWearSystem::new();
        for _ in 0..10_000 {
            sys.update(BrakeWearData {
                wheel: 1,
                brake_torque: 3000.0,
                brake_pressure: 80.0,
                wheel_speed: 100.0,
                vehicle_speed: 40.0,
                ..Default::default()
            });
        }
        assert!(sys.pad_wear_state().pad_thickness[1] < 10.0);
        assert!(sys.pad_wear_state().remaining_life(1) < 1.0);
    }

    #[test]
    fn braking_multiplier_stays_bounded() {
        let sys = BrakeWearSystem::new();
        for wheel in 0..4 {
            let m = sys.braking_multiplier(wheel);
            assert!((0.1..=1.0).contains(&m));
        }
    }

    #[test]
    fn brake_torque_uses_effective_friction() {
        let sys = BrakeWearSystem::new();
        let t = sys.calculate_brake_torque(50.0, 0);
        assert!(t > 0.0);
    }

    #[test]
    fn reset_restores_defaults() {
        let mut sys = BrakeWearSystem::new();
        sys.update(BrakeWearData {
            wheel: 2,
            brake_torque: 3000.0,
            brake_pressure: 80.0,
            wheel_speed: 100.0,
            vehicle_speed: 40.0,
            ..Default::default()
        });
        sys.reset();
        assert_eq!(sys.disc_temperature(2), 300.0);
        assert_eq!(sys.pad_wear_state().pad_thickness[2], 10.0);
        assert!(!sys.is_faded(2));
    }

    #[test]
    fn replacement_flags() {
        let mut sys = BrakeWearSystem::new();
        assert!(!sys.needs_pad_replacement(0));
        assert!(!sys.needs_disc_replacement(0));
        sys.pad_wear.pad_thickness[0] = 0.5;
        sys.disc_wear.disc_thickness[0] = 14.0;
        assert!(sys.needs_pad_replacement(0));
        assert!(sys.needs_disc_replacement(0));
    }
}
