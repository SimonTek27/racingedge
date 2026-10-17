//! Usura e temperatura degli pneumatici; mescole e flat spot.
//!
//! Port di `src/engine/physics/TireWearSystem.{h,cpp}` e
//! `src/engine/physics/TireFlatSpot.h` di ksengine.

// ── Modello termico (TireThermalState / TireThermalConfig) ──────────────────

/// Stato termico dello pneumatico (port di `TireThermalState`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TireThermalState {
    /// Temperatura superficiale interna/centrale/esterna, °C.
    pub surface_temp: [f32; 3],
    /// Temperatura del core, °C.
    pub core_temp: f32,
    /// Temperatura della carcassa, °C.
    pub carcass_temp: f32,
    /// Temperatura della cintura, °C.
    pub belt_temp: f32,
    /// Temperatura ambiente, °C.
    pub ambient_temp: f32,
    /// Temperatura della superficie stradale, °C.
    pub road_temp: f32,
}

impl Default for TireThermalState {
    fn default() -> Self {
        Self {
            surface_temp: [30.0, 30.0, 30.0],
            core_temp: 35.0,
            carcass_temp: 32.0,
            belt_temp: 31.0,
            ambient_temp: 25.0,
            road_temp: 30.0,
        }
    }
}

impl TireThermalState {
    /// Temperatura media della superficie (port di `averageSurfaceTemp`).
    pub fn average_surface_temp(&self) -> f32 {
        (self.surface_temp[0] + self.surface_temp[1] + self.surface_temp[2]) / 3.0
    }

    /// Gradiente da interno a esterno (port di `temperatureGradient`).
    pub fn temperature_gradient(&self) -> f32 {
        self.surface_temp[2] - self.surface_temp[0]
    }

    /// Se lo pneumatico è nella finestra ottimale (port di `isOptimalTemp`).
    pub fn is_optimal_temp(&self, optimal_min: f32, optimal_max: f32) -> bool {
        let avg = self.average_surface_temp();
        avg >= optimal_min && avg <= optimal_max
    }
}

/// Configurazione termica dello pneumatico (port di `TireThermalConfig`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TireThermalConfig {
    /// Velocità di scambio termico.
    pub thermal_conductivity: f32,
    /// Calore specifico, J/(kg·K).
    pub specific_heat: f32,
    /// Massa della gomma per pneumatico, kg.
    pub rubber_mass: f32,
    /// Area dell'impronta, m².
    pub contact_patch_area: f32,
    /// Coefficiente di convezione, W/(m²·K).
    pub convection_coeff: f32,
    /// Frazione di massa termica del core.
    pub core_heat_capacity: f32,
    /// Frazione di massa termica della carcassa.
    pub carcass_heat_capacity: f32,
    /// Minimo della finestra termica ottimale, °C.
    pub optimal_temp_min: f32,
    /// Massimo della finestra termica ottimale, °C.
    pub optimal_temp_max: f32,
    /// Temperatura critica, °C.
    pub critical_temp: f32,
    /// Inizio della finestra di graining, °C.
    pub graining_temp_min: f32,
    /// Fine della finestra di graining, °C.
    pub graining_temp_max: f32,
    /// Temperatura di blistering, °C.
    pub blistering_temp: f32,
}

impl Default for TireThermalConfig {
    fn default() -> Self {
        Self {
            thermal_conductivity: 0.15,
            specific_heat: 1200.0,
            rubber_mass: 8.0,
            contact_patch_area: 0.02,
            convection_coeff: 50.0,
            core_heat_capacity: 0.3,
            carcass_heat_capacity: 0.2,
            optimal_temp_min: 70.0,
            optimal_temp_max: 110.0,
            critical_temp: 140.0,
            graining_temp_min: 50.0,
            graining_temp_max: 80.0,
            blistering_temp: 130.0,
        }
    }
}

// ── Usura (TireWearDetailState / TireWearConfig) ────────────────────────────

/// Stato di usura dello pneumatico (port di `TireWearDetailState`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TireWearDetailState {
    /// Usura totale (`0` = nuovo, `1` = distrutto).
    pub wear: f32,
    /// Usura superficiale (graining/blistering).
    pub surface_wear: f32,
    /// Degrado strutturale.
    pub structural_wear: f32,
    /// Perdita di grip dovuta all'usura.
    pub grip_loss: f32,
    /// Livello di graining, `0..=1`.
    pub graining: f32,
    /// Livello di blistering, `0..=1`.
    pub blistering: f32,
    /// Grumi di gomma raccolti (pickup).
    pub clumps: f32,
}

impl TireWearDetailState {
    /// Moltiplicatore di grip effettivo (port di `gripMultiplier`).
    pub fn grip_multiplier(&self) -> f32 {
        let base_grip = 1.0 - self.wear * 0.4;
        let surface_effect = 1.0 - self.surface_wear * 0.3;
        let graining_effect = 1.0 - self.graining * 0.25;
        let blistering_effect = 1.0 - self.blistering * 0.35;
        (base_grip * surface_effect * graining_effect * blistering_effect).clamp(0.3, 1.0)
    }

    /// Usura in percentuale per la visualizzazione (port di `wearPercent`).
    pub fn wear_percent(&self) -> f32 {
        self.wear * 100.0
    }
}

/// Configurazione dell'usura (port di `TireWearConfig`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TireWearConfig {
    /// Usura base per secondo a pieno carico.
    pub base_wear_rate: f32,
    /// Esponente dell'usura rispetto al carico.
    pub load_wear_exponent: f32,
    /// Moltiplicatore dell'usura da slittamento.
    pub slip_wear_multiplier: f32,
    /// Esponente dell'usura rispetto alla temperatura.
    pub temperature_wear_exponent: f32,
    /// Temperatura di insorgenza del graining, °C.
    pub graining_onset_temp: f32,
    /// Temperatura di picco del graining, °C.
    pub graining_peak_temp: f32,
    /// Temperatura di uscita dal graining, °C.
    pub graining_offset_temp: f32,
    /// Temperatura di insorgenza del blistering, °C.
    pub blistering_onset_temp: f32,
    /// Velocità di accumulo dei grumi.
    pub clump_buildup_rate: f32,
    /// Velocità alla quale i grumi si staccano, m/s.
    pub clump_shed_speed: f32,
    /// Graining massimo.
    pub max_graining: f32,
    /// Blistering massimo.
    pub max_blistering: f32,
    /// Grumi massimi.
    pub max_clumps: f32,
}

impl Default for TireWearConfig {
    fn default() -> Self {
        Self {
            base_wear_rate: 0.001,
            load_wear_exponent: 1.2,
            slip_wear_multiplier: 2.0,
            temperature_wear_exponent: 1.5,
            graining_onset_temp: 50.0,
            graining_peak_temp: 65.0,
            graining_offset_temp: 80.0,
            blistering_onset_temp: 120.0,
            clump_buildup_rate: 0.0005,
            clump_shed_speed: 80.0,
            max_graining: 0.4,
            max_blistering: 0.5,
            max_clumps: 0.3,
        }
    }
}

// ── Mescole (TireCompound) ──────────────────────────────────────────────────

/// Mescola dello pneumatico (port di `TireCompound`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TireCompound {
    /// Super morbida.
    SuperSoft,
    /// Morbida.
    Soft,
    /// Media.
    #[default]
    Medium,
    /// Dura.
    Hard,
    /// Super dura.
    SuperHard,
    /// Da bagnato.
    Wet,
    /// Intermedia.
    Intermediate,
    /// Personalizzata.
    Custom,
}

/// Proprietà di una mescola (port di `TireCompoundData`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TireCompoundData {
    /// La mescola.
    pub compound: TireCompound,
    /// Grip relativo (`1.0` = base).
    pub grip_factor: f32,
    /// Resistenza all'usura (`1.0` = base).
    pub wear_resistance: f32,
    /// Modificatore di conduzione termica.
    pub thermal_conductivity: f32,
    /// Minimo della finestra ottimale, °C.
    pub optimal_temp_min: f32,
    /// Massimo della finestra ottimale, °C.
    pub optimal_temp_max: f32,
    /// Ampiezza della finestra ottimale, °C.
    pub operating_temp_range: f32,
    /// Nome della mescola.
    pub name: &'static str,
}

impl Default for TireCompoundData {
    fn default() -> Self {
        Self {
            compound: TireCompound::Medium,
            grip_factor: 1.0,
            wear_resistance: 1.0,
            thermal_conductivity: 1.0,
            optimal_temp_min: 70.0,
            optimal_temp_max: 110.0,
            operating_temp_range: 40.0,
            name: "Medium",
        }
    }
}

impl TireCompoundData {
    /// Le proprietà di una mescola (port di `getCompoundData`).
    pub fn for_compound(compound: TireCompound) -> Self {
        let (grip_factor, wear_resistance, thermal_conductivity, optimal_min, optimal_max, name) =
            match compound {
                TireCompound::SuperSoft => (1.15, 0.5, 1.0, 65.0, 95.0, "Super Soft"),
                TireCompound::Soft => (1.08, 0.7, 1.0, 70.0, 100.0, "Soft"),
                TireCompound::Medium => (1.0, 1.0, 1.0, 75.0, 110.0, "Medium"),
                TireCompound::Hard => (0.93, 1.4, 1.0, 80.0, 120.0, "Hard"),
                TireCompound::SuperHard => (0.87, 1.8, 1.0, 85.0, 130.0, "Super Hard"),
                TireCompound::Wet => (0.75, 1.2, 0.8, 40.0, 70.0, "Wet"),
                TireCompound::Intermediate => (0.85, 1.1, 1.0, 50.0, 85.0, "Intermediate"),
                TireCompound::Custom => (1.0, 1.0, 1.0, 70.0, 110.0, "Custom"),
            };
        Self {
            compound,
            grip_factor,
            wear_resistance,
            thermal_conductivity,
            optimal_temp_min: optimal_min,
            optimal_temp_max: optimal_max,
            operating_temp_range: optimal_max - optimal_min,
            name,
        }
    }
}

// ── Sistema di usura per ruota (TireWearSystem) ─────────────────────────────

/// Usura e temperatura di uno pneumatico (port di `TireWearSystem`).
#[derive(Clone, Debug)]
pub struct TireWearSystem {
    thermal_config: TireThermalConfig,
    wear_config: TireWearConfig,
    compound: TireCompoundData,
    thermal: TireThermalState,
    wear_detail: TireWearDetailState,
    track_grip: f32,
    distance_traveled: f32,
    slip_history: f32,
}

impl Default for TireWearSystem {
    fn default() -> Self {
        let mut system = Self {
            thermal_config: TireThermalConfig::default(),
            wear_config: TireWearConfig::default(),
            compound: TireCompoundData::default(),
            thermal: TireThermalState::default(),
            wear_detail: TireWearDetailState::default(),
            track_grip: 1.0,
            distance_traveled: 0.0,
            slip_history: 0.0,
        };
        system.reset();
        system
    }
}

impl TireWearSystem {
    /// Un sistema di usura nello stato iniziale.
    pub fn new() -> Self {
        Self::default()
    }

    /// Imposta le configurazioni termica e di usura (port di `setConfig`).
    pub fn set_config(&mut self, thermal_config: TireThermalConfig, wear_config: TireWearConfig) {
        self.thermal_config = thermal_config;
        self.wear_config = wear_config;
    }

    /// Imposta la mescola da un preset (port di `setCompound(TireCompound)`).
    pub fn set_compound(&mut self, compound: TireCompound) {
        self.compound = TireCompoundData::for_compound(compound);
    }

    /// Imposta le proprietà della mescola (port di `setCompound(data)`).
    pub fn set_compound_data(&mut self, data: TireCompoundData) {
        self.compound = data;
    }

    /// Le proprietà della mescola corrente.
    pub fn compound(&self) -> TireCompoundData {
        self.compound
    }

    /// Imposta la temperatura ambiente.
    pub fn set_ambient_temp(&mut self, temp: f32) {
        self.thermal.ambient_temp = temp;
    }

    /// Imposta la temperatura della strada.
    pub fn set_road_temp(&mut self, temp: f32) {
        self.thermal.road_temp = temp;
    }

    /// Imposta il grip della pista, `0..=1`.
    pub fn set_track_grip(&mut self, grip: f32) {
        self.track_grip = grip.clamp(0.1, 1.0);
    }

    /// Un passo di simulazione (port di `update`).
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        speed: f32,
        normal_load: f32,
        slip_angle: f32,
        slip_ratio: f32,
        lateral_force: f32,
        longitudinal_force: f32,
        dt: f32,
    ) {
        self.update_thermal(
            speed,
            slip_angle,
            slip_ratio,
            lateral_force,
            longitudinal_force,
            dt,
        );
        self.update_wear(
            normal_load,
            slip_angle,
            slip_ratio,
            lateral_force,
            longitudinal_force,
            dt,
        );
        self.update_graining(dt);
        self.update_blistering(dt);
    }

    /// Lo stato termico corrente.
    pub fn thermal_state(&self) -> TireThermalState {
        self.thermal
    }

    /// Lo stato di usura corrente.
    pub fn wear_state(&self) -> TireWearDetailState {
        self.wear_detail
    }

    /// Il moltiplicatore di grip corrente.
    pub fn grip_multiplier(&self) -> f32 {
        self.wear_detail.grip_multiplier()
    }

    /// Il mu efficace (port di `effectiveMu`).
    pub fn effective_mu(&self) -> f32 {
        let base_mu = self.compound.grip_factor;
        let wear_effect = self.wear_detail.grip_multiplier();
        let temp_effect = if self.thermal.is_optimal_temp(
            self.compound.optimal_temp_min,
            self.compound.optimal_temp_max,
        ) {
            1.0
        } else {
            0.9
        };
        base_mu * wear_effect * temp_effect * self.track_grip
    }

    /// Se serve un pit stop (port di `needsPitStop`).
    pub fn needs_pit_stop(&self) -> bool {
        self.wear_detail.wear > 0.85 || self.wear_detail.blistering > 0.4
    }

    /// Stima dei giri rimanenti (port di `estimatedLapsRemaining`).
    pub fn estimated_laps_remaining(&self) -> f32 {
        if self.wear_detail.wear >= 1.0 {
            return 0.0;
        }
        let wear_per_lap = self.wear_detail.wear / (self.distance_traveled / 5000.0).max(1.0);
        if wear_per_lap <= 0.0 {
            return 999.0;
        }
        (1.0 - self.wear_detail.wear) / wear_per_lap
    }

    /// Riporta lo stato allo stato iniziale (port di `reset`).
    pub fn reset(&mut self) {
        self.thermal = TireThermalState::default();
        self.wear_detail = TireWearDetailState::default();
        self.distance_traveled = 0.0;
        self.slip_history = 0.0;
    }

    fn update_thermal(
        &mut self,
        speed: f32,
        slip_angle: f32,
        slip_ratio: f32,
        lateral_force: f32,
        longitudinal_force: f32,
        dt: f32,
    ) {
        let heat_gen = self.calculate_heat_generation(
            slip_angle,
            slip_ratio,
            lateral_force,
            longitudinal_force,
        );
        let heat_loss = self.calculate_heat_loss(speed);
        let contact_temp = self.calculate_contact_patch_temp();

        let net_heat = (heat_gen - heat_loss) * dt
            / (self.thermal_config.specific_heat * self.thermal_config.rubber_mass);

        let heat_distribution = 0.3;

        self.thermal.surface_temp[0] +=
            (net_heat * (1.0 - heat_distribution) + contact_temp * 0.01) * dt;
        self.thermal.surface_temp[0] -=
            (self.thermal.surface_temp[0] - self.thermal.ambient_temp) * 0.05 * dt;

        self.thermal.surface_temp[1] += (net_heat * (1.0 - heat_distribution * 0.5)) * dt;
        self.thermal.surface_temp[1] -=
            (self.thermal.surface_temp[1] - self.thermal.ambient_temp) * 0.04 * dt;

        self.thermal.surface_temp[2] += (net_heat * (1.0 - heat_distribution)) * dt;
        self.thermal.surface_temp[2] -=
            (self.thermal.surface_temp[2] - self.thermal.ambient_temp) * 0.05 * dt;

        let core_target = self.thermal.average_surface_temp() * 0.7 + self.thermal.road_temp * 0.3;
        self.thermal.core_temp +=
            (core_target - self.thermal.core_temp) * self.thermal_config.core_heat_capacity * dt;

        let carcass_target =
            self.thermal.average_surface_temp() * 0.5 + self.thermal.core_temp * 0.5;
        self.thermal.carcass_temp += (carcass_target - self.thermal.carcass_temp)
            * self.thermal_config.carcass_heat_capacity
            * dt;

        for temp in &mut self.thermal.surface_temp {
            *temp = temp.clamp(self.thermal.ambient_temp - 10.0, 200.0);
        }
        self.thermal.core_temp = self
            .thermal
            .core_temp
            .clamp(self.thermal.ambient_temp, 180.0);
        self.thermal.carcass_temp = self
            .thermal
            .carcass_temp
            .clamp(self.thermal.ambient_temp, 160.0);
    }

    fn update_wear(
        &mut self,
        normal_load: f32,
        slip_angle: f32,
        slip_ratio: f32,
        _lateral_force: f32,
        _longitudinal_force: f32,
        dt: f32,
    ) {
        let load_factor = (normal_load / 5000.0).powf(self.wear_config.load_wear_exponent);

        let slip_magnitude = (slip_angle * slip_angle + slip_ratio * slip_ratio).sqrt();
        let slip_factor = 1.0 + slip_magnitude * self.wear_config.slip_wear_multiplier;

        let temp = self.thermal.average_surface_temp();
        let temp_optimal = (self.compound.optimal_temp_min + self.compound.optimal_temp_max) / 2.0;
        let temp_deviation = (temp - temp_optimal).abs() / self.compound.operating_temp_range;
        let temp_factor = 1.0 + temp_deviation.powf(self.wear_config.temperature_wear_exponent);

        let wear_resistance = self.compound.wear_resistance * self.track_grip;

        let wear_increment =
            self.wear_config.base_wear_rate * load_factor * slip_factor * temp_factor
                / wear_resistance
                * dt;

        self.wear_detail.wear = (self.wear_detail.wear + wear_increment).min(1.0);
        self.wear_detail.structural_wear = self
            .wear_detail
            .wear
            .min(self.wear_detail.structural_wear + wear_increment * 0.7);
        self.wear_detail.surface_wear = self
            .wear_detail
            .wear
            .min(self.wear_detail.surface_wear + wear_increment * 1.3);

        self.wear_detail.grip_loss = 1.0 - self.wear_detail.grip_multiplier();

        self.distance_traveled += normal_load * slip_magnitude * dt * 0.001;
    }

    fn update_graining(&mut self, dt: f32) {
        let temp = self.thermal.average_surface_temp();

        if temp > self.wear_config.graining_onset_temp
            && temp < self.wear_config.graining_offset_temp
        {
            let peak_temp = (self.wear_config.graining_onset_temp
                + self.wear_config.graining_offset_temp)
                / 2.0;
            let proximity = (1.0
                - (temp - peak_temp).abs()
                    / ((self.wear_config.graining_offset_temp
                        - self.wear_config.graining_onset_temp)
                        / 2.0))
                .clamp(0.0, 1.0);

            self.wear_detail.graining = self
                .wear_config
                .max_graining
                .min(self.wear_detail.graining + proximity * 0.01 * dt);
        } else {
            self.wear_detail.graining = (self.wear_detail.graining - 0.005 * dt).max(0.0);
        }
    }

    fn update_blistering(&mut self, dt: f32) {
        let temp = self.thermal.average_surface_temp();

        if temp > self.wear_config.blistering_onset_temp {
            let severity = ((temp - self.wear_config.blistering_onset_temp) / 30.0).clamp(0.0, 1.0);
            self.wear_detail.blistering = self
                .wear_config
                .max_blistering
                .min(self.wear_detail.blistering + severity * 0.02 * dt);
        } else {
            self.wear_detail.blistering = (self.wear_detail.blistering - 0.003 * dt).max(0.0);
        }
    }

    fn calculate_heat_generation(
        &self,
        slip_angle: f32,
        slip_ratio: f32,
        lateral_force: f32,
        longitudinal_force: f32,
    ) -> f32 {
        let lateral_work = lateral_force.abs() * slip_angle.abs();
        let longitudinal_work = longitudinal_force.abs() * slip_ratio.abs();
        let total_work =
            (lateral_work + longitudinal_work) * self.thermal_config.contact_patch_area;

        total_work * self.thermal_config.thermal_conductivity * self.compound.thermal_conductivity
    }

    fn calculate_heat_loss(&self, speed: f32) -> f32 {
        let area = self.thermal_config.contact_patch_area * 4.0;
        let delta_t = self.thermal.average_surface_temp() - self.thermal.ambient_temp;

        let convection = self.thermal_config.convection_coeff * (1.0 + speed * 0.02);
        convection * area * delta_t
    }

    fn calculate_contact_patch_temp(&self) -> f32 {
        let road_effect = self.thermal.road_temp * 0.6;
        let core_effect = self.thermal.core_temp * 0.3;
        let ambient_effect = self.thermal.ambient_temp * 0.1;
        road_effect + core_effect + ambient_effect
    }
}

// ── Flat spot (TireFlatSpot) ────────────────────────────────────────────────

/// Stato del flat spot (port di `TireFlatSpotState`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TireFlatSpotState {
    /// Gravità, `0..=1`.
    pub severity: f32,
    /// Angolo della zona piatta, rad.
    pub angle: f32,
    /// Vibrazione risultante.
    pub vibration: f32,
}

/// Flat spot su una ruota (port di `TireFlatSpot`).
#[derive(Clone, Copy, Debug, Default)]
pub struct TireFlatSpot {
    /// Stato corrente.
    pub state: TireFlatSpotState,
}

impl TireFlatSpot {
    /// Un flat spot nuovo.
    pub fn new() -> Self {
        Self::default()
    }

    /// Un passo di aggiornamento (port di `update`).
    pub fn update(&mut self, locked: bool, speed_ms: f32, load_n: f32, dt: f32) {
        if locked && speed_ms > 10.0 {
            let rate = (speed_ms / 60.0) * (load_n / 4000.0);
            self.state.severity = (self.state.severity + rate * dt * 0.25).clamp(0.0, 1.0);
        } else {
            self.state.severity = (self.state.severity - dt * 0.001).max(0.0);
        }
        let wheel_freq = speed_ms / (2.0 * std::f32::consts::PI * 0.33);
        self.state.vibration = self.state.severity * (wheel_freq / 20.0).clamp(0.0, 1.0);
    }

    /// Penalità di grip (port di `gripPenalty`).
    pub fn grip_penalty(&self) -> f32 {
        1.0 - self.state.severity * 0.18
    }

    /// Forza di vibrazione (port di `vibrationForce`).
    pub fn vibration_force(&self, speed_ms: f32) -> f32 {
        let wheel_freq = speed_ms / (2.0 * std::f32::consts::PI * 0.33);
        self.state.severity * 400.0 * (wheel_freq / 25.0).clamp(0.0, 1.0)
    }

    /// Azzera il flat spot (port di `reset`).
    pub fn reset(&mut self) {
        self.state = TireFlatSpotState::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn heat(wheel: &mut TireWearSystem, steps: usize) {
        for _ in 0..steps {
            wheel.update(60.0, 4000.0, 0.08, 0.1, 3000.0, 2000.0, 0.016);
        }
    }

    #[test]
    fn more_slip_generates_more_heat() {
        // Il riferimento dissipa per convezione verso l'ambiente: la
        // temperatura sale solo se la generazione di calore la supera, quindi
        // confrontiamo un caso con forze elevate contro uno a forze nulle.
        let mut idle = TireWearSystem::new();
        let mut sliding = TireWearSystem::new();
        for _ in 0..2000 {
            idle.update(60.0, 4000.0, 0.0, 0.0, 0.0, 0.0, 0.016);
            sliding.update(60.0, 4000.0, 0.1, 0.1, 5.0e6, 5.0e6, 0.016);
        }
        assert!(
            sliding.thermal_state().average_surface_temp()
                > idle.thermal_state().average_surface_temp()
        );
    }

    #[test]
    fn wear_accumulates_and_lowers_grip() {
        let mut wheel = TireWearSystem::new();
        heat(&mut wheel, 4000);
        assert!(wheel.wear_state().wear > 0.0);
        assert!(wheel.grip_multiplier() <= 1.0);
        assert!(wheel.effective_mu() <= wheel.compound().grip_factor + f32::EPSILON);
    }

    #[test]
    fn grip_multiplier_is_floor_capped() {
        let mut detail = TireWearDetailState {
            wear: 1.0,
            surface_wear: 1.0,
            graining: 1.0,
            blistering: 1.0,
            ..Default::default()
        };
        assert_eq!(detail.grip_multiplier(), 0.3);
        detail.wear = 0.0;
        assert!(detail.grip_multiplier() >= 0.3);
    }

    #[test]
    fn compound_presets_match_reference() {
        let soft = TireCompoundData::for_compound(TireCompound::Soft);
        assert_eq!(soft.name, "Soft");
        assert!((soft.grip_factor - 1.08).abs() < 1e-6);
        assert!((soft.operating_temp_range - 30.0).abs() < 1e-6);

        let wet = TireCompoundData::for_compound(TireCompound::Wet);
        assert_eq!(wet.name, "Wet");
        assert!((wet.thermal_conductivity - 0.8).abs() < 1e-6);
    }

    #[test]
    fn set_compound_changes_mu_baseline() {
        let mut wheel = TireWearSystem::new();
        wheel.set_compound(TireCompound::SuperSoft);
        assert_eq!(wheel.compound().name, "Super Soft");
        assert!((wheel.compound().grip_factor - 1.15).abs() < 1e-6);
    }

    #[test]
    fn optimal_temp_window() {
        let state = TireThermalState {
            surface_temp: [90.0, 90.0, 90.0],
            ..Default::default()
        };
        assert!(state.is_optimal_temp(70.0, 110.0));
        assert!(!state.is_optimal_temp(120.0, 150.0));
    }

    #[test]
    fn graining_and_blistering_grow_in_range() {
        let mut wheel = TireWearSystem::new();
        wheel.thermal.surface_temp = [65.0, 65.0, 65.0];
        let g0 = wheel.wear_state().graining;
        for _ in 0..1000 {
            wheel.update(20.0, 1000.0, 0.0, 0.0, 0.0, 0.0, 0.016);
        }
        assert!(wheel.wear_state().graining >= g0);

        wheel.thermal.surface_temp = [160.0, 160.0, 160.0];
        for _ in 0..1000 {
            wheel.update(20.0, 1000.0, 0.0, 0.0, 0.0, 0.0, 0.016);
        }
        assert!(wheel.wear_state().blistering > 0.0);
    }

    #[test]
    fn needs_pit_stop_when_worn() {
        let mut wheel = TireWearSystem::new();
        assert!(!wheel.needs_pit_stop());
        wheel.wear_detail.wear = 0.9;
        assert!(wheel.needs_pit_stop());
    }

    #[test]
    fn reset_clears_state() {
        let mut wheel = TireWearSystem::new();
        heat(&mut wheel, 1000);
        wheel.reset();
        assert_eq!(wheel.wear_state().wear, 0.0);
        assert_eq!(wheel.thermal_state().average_surface_temp(), 30.0);
    }

    #[test]
    fn flat_spot_forms_when_locked() {
        let mut flat = TireFlatSpot::new();
        for _ in 0..5000 {
            flat.update(true, 40.0, 4000.0, 0.016);
        }
        assert!(flat.state.severity > 0.0);
        assert!(flat.grip_penalty() < 1.0);
        assert!(flat.vibration_force(40.0) > 0.0);
    }

    #[test]
    fn flat_spot_heals_over_time() {
        let mut flat = TireFlatSpot::new();
        flat.state.severity = 0.5;
        for _ in 0..40_000 {
            flat.update(false, 0.0, 0.0, 0.016);
        }
        assert_eq!(flat.state.severity, 0.0);
    }
}
