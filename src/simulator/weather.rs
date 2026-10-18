//! Meteo — evoluzione dello stato, effetti su grip e forze del vento.
//!
//! Port di ksengine `WeatherPhysics.{h,cpp}`, `WeatherConfig.{h,cpp}` (il
//! `.cpp` upstream è uno stub solo-Qt: qui la conversione config→stato è
//! completa) e dei preset di `src/simulator/WeatherControl.h`. `WeatherState`
//! è definito in `PhysicsCoreTypes.h` ed è condiviso dagli eventi audio di
//! pista (`track_audio`), così meteo e audio leggono lo stesso tipo.

use bevy::math::Vec3;

/// Densità dell'aria standard a livello del mare (kg/m³).
pub const DEFAULT_AIR_DENSITY: f32 = 1.225;

/// Stato meteo (port di `physics::WeatherState`).
#[derive(Clone, Copy, Debug)]
pub struct WeatherState {
    /// Temperatura ambiente (°C).
    pub ambient_temp: f32,
    /// Temperatura della superficie di pista (°C).
    pub track_temp: f32,
    /// Densità dell'aria (kg/m³).
    pub air_density: f32,
    /// Bagnato della pista (0 = asciutto, 1 = bagnato).
    pub track_wetness: f32,
    /// Intensità pioggia (mm/h).
    pub rain_intensity: f32,
    /// Velocità del vento (m/s).
    pub wind_speed: f32,
    /// Direzione del vento in gradi (0 = Nord).
    pub wind_direction: f32,
    /// Umidità relativa (0..1).
    pub humidity: f32,
    /// Copertura nuvolosa (0..1).
    pub cloud_cover: f32,
}

impl Default for WeatherState {
    fn default() -> Self {
        Self {
            ambient_temp: 26.0,
            track_temp: 30.0,
            air_density: DEFAULT_AIR_DENSITY,
            track_wetness: 0.0,
            rain_intensity: 0.0,
            wind_speed: 0.0,
            wind_direction: 0.0,
            humidity: 0.5,
            cloud_cover: 0.0,
        }
    }
}

impl WeatherState {
    /// Riduzione di grip per il bagnato (port di `gripReduction`).
    pub fn grip_reduction(&self) -> f32 {
        let risk = self.aquaplaning_risk();
        (self.track_wetness * 0.3 + risk * 0.2).clamp(0.0, 0.8)
    }

    /// Rischio di aquaplaning 0..1 (port di `aquaplaningRisk`).
    pub fn aquaplaning_risk(&self) -> f32 {
        (self.track_wetness * 0.5 + self.rain_intensity * 0.001).clamp(0.0, 1.0)
    }

    /// Condizioni adatte a gomme da asciutto (port di `isDry`).
    pub fn is_dry(&self) -> bool {
        self.track_wetness < 0.1 && self.rain_intensity < 0.1
    }

    /// Condizioni che richiedono gomme da bagnato (port di `isWet`).
    pub fn is_wet(&self) -> bool {
        self.track_wetness > 0.3 || self.rain_intensity > 0.5
    }
}

/// Effetti meteo consumati dal loop veicolo (port di `WeatherEffects`).
#[derive(Clone, Copy, Debug)]
pub struct WeatherEffects {
    pub aquaplaning_risk: f32,
    pub track_grip_reduction: f32,
    pub wind_force: Vec3,
    pub air_density: f32,
}

impl Default for WeatherEffects {
    fn default() -> Self {
        Self {
            aquaplaning_risk: 0.0,
            track_grip_reduction: 0.0,
            wind_force: Vec3::ZERO,
            air_density: DEFAULT_AIR_DENSITY,
        }
    }
}

/// Configurazione meteo (port di `WeatherConfig`).
#[derive(Clone, Debug)]
pub struct WeatherConfig {
    pub ambient_temp_c: f32,
    pub track_temp_c: f32,
    pub humidity: f32,
    pub wind_speed_ms: f32,
    pub wind_dir_deg: f32,
    pub rain_intensity: f32,
    pub fog_density: f32,
    pub preset_name: String,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            ambient_temp_c: 25.0,
            track_temp_c: 30.0,
            humidity: 0.5,
            wind_speed_ms: 0.0,
            wind_dir_deg: 0.0,
            rain_intensity: 0.0,
            fog_density: 0.0,
            preset_name: "clear".to_string(),
        }
    }
}

/// Converte una config nel relativo stato meteo (port di `weatherStateFromConfig`).
pub fn weather_state_from_config(c: &WeatherConfig) -> WeatherState {
    WeatherState {
        ambient_temp: c.ambient_temp_c,
        track_temp: c.track_temp_c,
        humidity: c.humidity,
        wind_speed: c.wind_speed_ms,
        wind_direction: c.wind_dir_deg,
        rain_intensity: c.rain_intensity,
        ..Default::default()
    }
}

/// Simulatore meteo leggero (port di `WeatherSimulator`).
#[derive(Clone, Debug)]
pub struct WeatherSimulator {
    state: WeatherState,
    effects: WeatherEffects,
    running: bool,
    time_multiplier: f32,
}

impl Default for WeatherSimulator {
    fn default() -> Self {
        Self {
            state: WeatherState::default(),
            effects: WeatherEffects::default(),
            running: false,
            time_multiplier: 1.0,
        }
    }
}

impl WeatherSimulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Imposta lo stato e ricalcola gli effetti (port di `setState`).
    pub fn set_state(&mut self, s: WeatherState) {
        self.state = s;
        self.state.air_density =
            DEFAULT_AIR_DENSITY * (288.15 / (273.15 + s.ambient_temp).max(1e-3));
        self.recompute_effects();
    }

    pub fn state(&self) -> &WeatherState {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut WeatherState {
        &mut self.state
    }

    pub fn effects(&self) -> &WeatherEffects {
        &self.effects
    }

    pub fn set_time_multiplier(&mut self, m: f32) {
        self.time_multiplier = m;
    }

    pub fn time_multiplier(&self) -> f32 {
        self.time_multiplier
    }

    pub fn start(&mut self) {
        self.running = true;
    }

    pub fn stop(&mut self) {
        self.running = false;
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    /// Riporta il meteo allo stato predefinito (port di `reset`).
    pub fn reset(&mut self) {
        self.state = WeatherState::default();
        self.recompute_effects();
    }

    /// Avanza il meteo: accumulo pioggia, asciugatura pista, vento (port di `update`).
    pub fn update(&mut self, dt: f32) {
        if !self.running || dt <= 0.0 {
            return;
        }
        let dt = (dt * self.time_multiplier).clamp(1e-4, 1.0);

        if self.state.rain_intensity > 0.05 {
            self.state.track_wetness = (self.state.track_wetness
                + self.state.rain_intensity * 0.00015 * dt)
                .clamp(0.0, 1.0);
        } else {
            self.state.track_wetness = (self.state.track_wetness - 0.002 * dt).max(0.0);
        }

        self.state.track_temp +=
            (self.state.ambient_temp + 5.0 - self.state.track_temp) * 0.01 * dt;

        self.state.air_density =
            DEFAULT_AIR_DENSITY * (288.15 / (273.15 + self.state.ambient_temp));

        self.recompute_effects();
    }

    /// Ricalcola gli effetti dallo stato (port di `recomputeEffects`).
    pub fn recompute_effects(&mut self) {
        self.effects.aquaplaning_risk = self.state.aquaplaning_risk();
        self.effects.track_grip_reduction = self.state.grip_reduction();
        self.effects.air_density = self.state.air_density;
        let rad = self.state.wind_direction.to_radians();
        let w = self.state.wind_speed;
        self.effects.wind_force = Vec3::new(rad.sin() * w, 0.0, -rad.cos() * w);
    }

    /// Preset: asciutto (port di `applyDry`).
    pub fn apply_dry(&mut self) {
        self.state.rain_intensity = 0.0;
        self.state.track_wetness = 0.0;
        self.state.humidity = 0.4;
        self.state.cloud_cover = 0.1;
        self.recompute_effects();
    }

    /// Preset: pioggia leggera (port di `applyLightRain`).
    pub fn apply_light_rain(&mut self) {
        self.state.rain_intensity = 2.0;
        self.state.track_wetness = 0.25;
        self.state.humidity = 0.8;
        self.state.cloud_cover = 0.7;
        self.recompute_effects();
    }

    /// Preset: pioggia intensa (port di `applyHeavyRain`).
    pub fn apply_heavy_rain(&mut self) {
        self.state.rain_intensity = 12.0;
        self.state.track_wetness = 0.7;
        self.state.humidity = 0.95;
        self.state.cloud_cover = 1.0;
        self.recompute_effects();
    }

    /// Preset: pista bagnata senza pioggia (port di `applyWetTrack`).
    pub fn apply_wet_track(&mut self) {
        self.state.rain_intensity = 0.0;
        self.state.track_wetness = 0.5;
        self.recompute_effects();
    }

    /// Preset: freddo (port di `applyCold`).
    pub fn apply_cold(&mut self) {
        self.state.ambient_temp = 5.0;
        self.state.track_temp = 8.0;
        self.recompute_effects();
    }
}

// =============================================================================
// PRESET DI CONTROLLO (WeatherControl.h)
// =============================================================================

/// Preset meteo pronto all'uso (port di `WeatherPreset`).
#[derive(Clone, Debug)]
pub struct WeatherPreset {
    pub id: String,
    pub name: String,
    pub ambient_c: f32,
    pub track_c: f32,
    pub wetness: f32,
    pub rain: f32,
    pub wind_ms: f32,
    pub wind_dir_deg: f32,
    pub fog: f32,
    pub cloud: f32,
}

impl Default for WeatherPreset {
    fn default() -> Self {
        Self {
            id: "dry".to_string(),
            name: "Dry".to_string(),
            ambient_c: 22.0,
            track_c: 28.0,
            wetness: 0.0,
            rain: 0.0,
            wind_ms: 0.0,
            wind_dir_deg: 0.0,
            fog: 0.0,
            cloud: 0.2,
        }
    }
}

/// Preset asciutto (port di `presetDry`).
pub fn preset_dry() -> WeatherPreset {
    WeatherPreset {
        id: "dry".to_string(),
        name: "Dry".to_string(),
        ambient_c: 26.0,
        track_c: 32.0,
        ..Default::default()
    }
}

/// Preset umido (port di `presetDamp`).
pub fn preset_damp() -> WeatherPreset {
    WeatherPreset {
        id: "damp".to_string(),
        name: "Damp".to_string(),
        ambient_c: 18.0,
        track_c: 20.0,
        wetness: 0.35,
        rain: 0.05,
        cloud: 0.7,
        ..Default::default()
    }
}

/// Preset bagnato (port di `presetWet`).
pub fn preset_wet() -> WeatherPreset {
    WeatherPreset {
        id: "wet".to_string(),
        name: "Wet".to_string(),
        ambient_c: 14.0,
        track_c: 15.0,
        wetness: 0.85,
        rain: 0.6,
        cloud: 0.95,
        fog: 0.15,
        ..Default::default()
    }
}

/// Preset per nome (port di `presetByName`): contiene "wet" → bagnato,
/// contiene "damp" → umido, altrimenti asciutto.
pub fn preset_by_name(name: &str) -> WeatherPreset {
    let name = name.to_ascii_lowercase();
    if name.contains("wet") {
        preset_wet()
    } else if name.contains("damp") {
        preset_damp()
    } else {
        preset_dry()
    }
}

/// Stato ora del giorno (port di `TimeOfDayState`).
#[derive(Clone, Copy, Debug)]
pub struct TimeOfDayState {
    pub hours: f32,
}

impl Default for TimeOfDayState {
    fn default() -> Self {
        Self { hours: 12.0 }
    }
}

impl TimeOfDayState {
    /// Imposta l'ora, avvolta in [0, 24) (port di `setHours`).
    pub fn set_hours(&mut self, h: f32) {
        self.hours = h.max(0.0) % 24.0;
    }

    /// Fattore di luce 0..1 (port di `daylight`).
    pub fn daylight(&self) -> f32 {
        use std::f32::consts::PI;
        if self.hours < 5.0 || self.hours > 21.0 {
            return 0.05;
        }
        if (6.0..=18.0).contains(&self.hours) {
            let t = (self.hours - 6.0) / 12.0;
            return 0.3 + 0.7 * (t * PI).sin();
        }
        0.15
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_state_is_dry() {
        let s = WeatherState::default();
        assert!(s.is_dry());
        assert!(!s.is_wet());
        assert_eq!(s.aquaplaning_risk(), 0.0);
        assert_eq!(s.grip_reduction(), 0.0);
        assert_eq!(s.air_density, DEFAULT_AIR_DENSITY);
    }

    #[test]
    fn grip_reduction_and_aquaplaning() {
        let s = WeatherState {
            track_wetness: 1.0,
            rain_intensity: 12.0,
            ..Default::default()
        };
        assert!(s.aquaplaning_risk() > 0.5);
        assert!(s.grip_reduction() > 0.0);
        assert!(s.grip_reduction() <= 0.8);
        assert!(s.is_wet());
        assert!(!s.is_dry());
    }

    #[test]
    fn update_accumulates_rain_wetness() {
        let mut sim = WeatherSimulator::new();
        sim.start();
        sim.state_mut().rain_intensity = 12.0;
        sim.update(1.0);
        assert!(sim.state().track_wetness > 0.0);
    }

    #[test]
    fn update_dries_the_track() {
        let mut sim = WeatherSimulator::new();
        sim.start();
        sim.state_mut().track_wetness = 0.5;
        sim.update(1.0);
        assert!((sim.state().track_wetness - 0.498).abs() < 1e-6);
    }

    #[test]
    fn update_is_a_noop_while_stopped() {
        let mut sim = WeatherSimulator::new();
        sim.state_mut().track_wetness = 0.5;
        sim.update(10.0);
        assert_eq!(sim.state().track_wetness, 0.5);
    }

    #[test]
    fn time_multiplier_scales_update() {
        let mut fast = WeatherSimulator::new();
        fast.start();
        fast.state_mut().rain_intensity = 12.0;
        fast.update(1.0);

        let mut slow = WeatherSimulator::new();
        slow.start();
        slow.set_time_multiplier(0.5);
        slow.state_mut().rain_intensity = 12.0;
        slow.update(1.0);

        assert!(fast.state().track_wetness > slow.state().track_wetness);
    }

    #[test]
    fn track_temp_converges_toward_ambient() {
        let mut sim = WeatherSimulator::new();
        sim.start();
        sim.set_state(WeatherState {
            ambient_temp: 20.0,
            track_temp: 30.0,
            ..Default::default()
        });
        sim.update(1.0);
        assert!(sim.state().track_temp < 30.0);
        assert!(sim.state().track_temp > 25.0, "single step is gradual");
    }

    #[test]
    fn air_density_drops_with_heat() {
        let mut sim = WeatherSimulator::new();
        sim.set_state(WeatherState {
            ambient_temp: 30.0,
            ..Default::default()
        });
        assert!(sim.state().air_density < DEFAULT_AIR_DENSITY);
        sim.set_state(WeatherState {
            ambient_temp: 15.0,
            ..Default::default()
        });
        assert!((sim.state().air_density - DEFAULT_AIR_DENSITY).abs() < 1e-3);
    }

    #[test]
    fn wind_force_follows_direction() {
        let mut sim = WeatherSimulator::new();
        sim.set_state(WeatherState {
            wind_speed: 10.0,
            wind_direction: 0.0,
            ..Default::default()
        });
        let f = sim.effects().wind_force;
        assert!(f.z < 0.0, "0 deg points to -Z (North)");
        assert!(f.x.abs() < 1e-3);

        sim.set_state(WeatherState {
            wind_speed: 10.0,
            wind_direction: 90.0,
            ..Default::default()
        });
        let f = sim.effects().wind_force;
        assert!((f.x - 10.0).abs() < 1e-3);
        assert!(f.z.abs() < 1e-3);
    }

    #[test]
    fn presets_set_expected_values() {
        let mut sim = WeatherSimulator::new();
        sim.apply_heavy_rain();
        assert_eq!(sim.state().rain_intensity, 12.0);
        assert_eq!(sim.state().track_wetness, 0.7);
        assert!(sim.effects().track_grip_reduction > 0.0);

        sim.apply_dry();
        assert_eq!(sim.state().track_wetness, 0.0);
        assert_eq!(sim.effects().track_grip_reduction, 0.0);

        sim.apply_cold();
        assert_eq!(sim.state().ambient_temp, 5.0);
        assert_eq!(sim.state().track_temp, 8.0);
    }

    #[test]
    fn reset_restores_defaults() {
        let mut sim = WeatherSimulator::new();
        sim.apply_heavy_rain();
        sim.reset();
        assert!(sim.state().is_dry());
        assert_eq!(sim.state().track_wetness, 0.0);
    }

    #[test]
    fn config_maps_to_state() {
        let cfg = WeatherConfig {
            ambient_temp_c: 12.0,
            track_temp_c: 14.0,
            humidity: 0.9,
            wind_speed_ms: 7.0,
            wind_dir_deg: 180.0,
            rain_intensity: 3.0,
            ..Default::default()
        };
        let s = weather_state_from_config(&cfg);
        assert_eq!(s.ambient_temp, 12.0);
        assert_eq!(s.track_temp, 14.0);
        assert_eq!(s.wind_speed, 7.0);
        assert_eq!(s.wind_direction, 180.0);
        assert_eq!(s.rain_intensity, 3.0);
    }

    #[test]
    fn preset_lookup_by_name() {
        assert_eq!(preset_by_name("HEAVY WET").id, "wet");
        assert_eq!(preset_by_name("Damp track").id, "damp");
        assert_eq!(preset_by_name("clear").id, "dry");
        assert_eq!(preset_by_name("clear").track_c, 32.0);
    }

    #[test]
    fn time_of_day_daylight_curve() {
        let mut t = TimeOfDayState::default();
        assert!((t.daylight() - 1.0).abs() < 1e-6, "noon is brightest");

        t.set_hours(0.0);
        assert_eq!(t.daylight(), 0.05);
        t.set_hours(19.0);
        assert_eq!(t.daylight(), 0.15);
        t.set_hours(25.0);
        assert!((t.hours - 1.0).abs() < 1e-6, "hours wrap");
    }
}
