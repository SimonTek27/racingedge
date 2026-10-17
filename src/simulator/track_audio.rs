//! Audio di pista: eventi posizionati pilotati da `audio_config.ini`.
//!
//! Port di `src/simulator/TrackAudioManager.{h,cpp}` di ksengine. L'originale
//! lascia `loadConfig` come segnaposto, quindi qui il formato ini è
//! implementato per intero (una sezione per evento):
//!
//! ```ini
//! [crowd]
//! type = ambient            ; campo libero
//! trigger_variable = RAIN   ; SUN | RAIN | WIND (vuoto = sempre)
//! trigger_condition = gt    ; lt | gt | eq
//! trigger_value = 0.2
//! loop = 1
//! camera_interior = 0.5
//! camera_exterior = 0.8
//! camera_track = 1.0
//! attenuation = inverse     ; inverse | linear | logarithmic
//! min_distance = 2
//! max_distance = 120
//! ```
//!
//! `WeatherState` è il sottoinsieme dei campi meteo che serve ai trigger
//! (l'attacco al meteo completo di B9 avviene a livello app).

use super::audio3d::distance_gain;

/// Modello di attenuazione per la riproduzione posizionata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AttenuationModel {
    #[default]
    Inverse,
    Linear,
    Logarithmic,
}

fn parse_attenuation(s: &str) -> AttenuationModel {
    match s.trim().to_ascii_lowercase().as_str() {
        "linear" => AttenuationModel::Linear,
        "logarithmic" => AttenuationModel::Logarithmic,
        _ => AttenuationModel::Inverse,
    }
}

/// Modalità camera di `SimulatorAudio::CameraMode` (include TV/Replay).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMode {
    Cockpit,
    Chase,
    Free,
    Tv,
    Replay,
}

pub use crate::simulator::weather::WeatherState;

/// Configurazione di un evento audio di pista.
#[derive(Clone, Debug)]
pub struct AudioEventConfig {
    pub name: String,
    pub kind: String,
    /// Variabile trigger (`SUN`/`RAIN`/`WIND`); vuota = sempre attivo.
    pub trigger_variable: String,
    /// `lt` | `gt` | `eq`.
    pub trigger_condition: String,
    pub trigger_value: f32,
    pub loop_event: bool,
    pub camera_interior_mult: f32,
    pub camera_exterior_mult: f32,
    pub camera_track_mult: f32,
    pub attenuation: AttenuationModel,
    pub min_distance: f32,
    pub max_distance: f32,
}

impl Default for AudioEventConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            kind: String::new(),
            trigger_variable: String::new(),
            trigger_condition: String::new(),
            trigger_value: 0.0,
            loop_event: false,
            camera_interior_mult: 1.0,
            camera_exterior_mult: 1.0,
            camera_track_mult: 1.0,
            attenuation: AttenuationModel::Inverse,
            min_distance: 1.0,
            max_distance: 120.0,
        }
    }
}

/// Evento a runtime: configurazione + stato di riproduzione.
#[derive(Clone, Debug)]
pub struct TrackAudioEvent {
    pub config: AudioEventConfig,
    pub last_trigger_value: f32,
    pub is_playing: bool,
    pub playback_position: f32,
    pub timer: f32,
}

impl TrackAudioEvent {
    pub fn new(config: AudioEventConfig) -> Self {
        Self {
            config,
            last_trigger_value: 0.0,
            is_playing: false,
            playback_position: 0.0,
            timer: 0.0,
        }
    }

    /// Moltiplicatore di gain per la camera corrente (port di
    /// `applyCameraMultiplier`).
    pub fn camera_multiplier(&self, mode: CameraMode) -> f32 {
        match mode {
            CameraMode::Cockpit => self.config.camera_interior_mult,
            CameraMode::Chase | CameraMode::Free => self.config.camera_exterior_mult,
            CameraMode::Tv | CameraMode::Replay => self.config.camera_track_mult,
        }
    }

    /// Se l'evento deve suonare, dato il valore della variabile trigger.
    fn should_trigger(&self, tv: f32) -> bool {
        if self.config.trigger_variable.is_empty() {
            return true;
        }
        match self.config.trigger_condition.as_str() {
            "lt" => tv < self.config.trigger_value,
            "gt" => tv > self.config.trigger_value,
            "eq" => (tv - self.config.trigger_value).abs() < 0.01,
            _ => false,
        }
    }
}

/// Gestore degli eventi audio di pista (port di `TrackAudioManager`).
#[derive(Clone, Debug, Default)]
pub struct TrackAudioManager {
    events: Vec<TrackAudioEvent>,
}

impl TrackAudioManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Carica gli eventi da un file; file assente = nessun evento.
    pub fn load_config_file(&mut self, path: &std::path::Path) -> bool {
        match std::fs::read_to_string(path) {
            Ok(text) => self.load_config(&text),
            Err(_) => false,
        }
    }

    /// Analizza il testo `audio_config.ini`. Ritorna `false` se non contiene
    /// sezioni.
    pub fn load_config(&mut self, text: &str) -> bool {
        self.events.clear();
        let mut current: Option<AudioEventConfig> = None;

        for raw in text.lines() {
            let line = raw
                .split([';', '#'])
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                if let Some(cfg) = current.take() {
                    self.events.push(TrackAudioEvent::new(cfg));
                }
                current = Some(AudioEventConfig {
                    name: line[1..line.len() - 1].trim().to_string(),
                    ..Default::default()
                });
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let Some(cfg) = current.as_mut() else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            match key.as_str() {
                "type" => cfg.kind = value.to_string(),
                "trigger_variable" => cfg.trigger_variable = value.to_string(),
                "trigger_condition" => cfg.trigger_condition = value.to_ascii_lowercase(),
                "trigger_value" => cfg.trigger_value = parse_f32(value, 0.0),
                "loop" => cfg.loop_event = parse_bool(value),
                "camera_interior" => cfg.camera_interior_mult = parse_f32(value, 1.0),
                "camera_exterior" => cfg.camera_exterior_mult = parse_f32(value, 1.0),
                "camera_track" => cfg.camera_track_mult = parse_f32(value, 1.0),
                "attenuation" => cfg.attenuation = parse_attenuation(value),
                "min_distance" => cfg.min_distance = parse_f32(value, 1.0),
                "max_distance" => cfg.max_distance = parse_f32(value, 120.0),
                _ => {}
            }
        }
        if let Some(cfg) = current.take() {
            self.events.push(TrackAudioEvent::new(cfg));
        }
        !self.events.is_empty()
    }

    /// Valore della variabile trigger dal meteo (port di `deriveTriggerValue`).
    pub fn derive_trigger_value(variable: &str, weather: &WeatherState) -> f32 {
        match variable.to_ascii_uppercase().as_str() {
            "SUN" => weather.cloud_cover,
            "RAIN" => weather.rain_intensity,
            "WIND" => (weather.wind_speed / 30.0).min(1.0),
            _ => 0.0,
        }
    }

    /// Avanza lo stato di tutti gli eventi (port di `update`).
    pub fn update(&mut self, dt: f32, weather: &WeatherState, _camera: CameraMode) {
        for ev in &mut self.events {
            let tv = Self::derive_trigger_value(&ev.config.trigger_variable, weather);
            ev.last_trigger_value = tv;
            let should = ev.should_trigger(tv);
            if should {
                if !ev.is_playing {
                    ev.is_playing = true;
                    ev.timer = 0.0;
                }
                ev.timer += dt;
                if ev.config.loop_event {
                    ev.playback_position = (ev.timer / 10.0) % 1.0;
                }
            } else {
                ev.is_playing = false;
            }
        }
    }

    pub fn events(&self) -> &[TrackAudioEvent] {
        &self.events
    }

    pub fn event(&self, name: &str) -> Option<&TrackAudioEvent> {
        self.events.iter().find(|e| e.config.name == name)
    }

    /// Gain per distanza, in base al modello di attenuazione dell'evento.
    pub fn attenuation_gain(&self, name: &str, distance: f32) -> f32 {
        let Some(ev) = self.event(name) else {
            return 0.0;
        };
        match ev.config.attenuation {
            AttenuationModel::Inverse => {
                distance_gain(distance, ev.config.min_distance, ev.config.max_distance)
            }
            AttenuationModel::Linear => {
                if distance <= ev.config.min_distance {
                    1.0
                } else if distance >= ev.config.max_distance {
                    0.0
                } else {
                    1.0 - (distance - ev.config.min_distance)
                        / (ev.config.max_distance - ev.config.min_distance)
                }
            }
            AttenuationModel::Logarithmic => {
                let d = distance.max(ev.config.min_distance);
                1.0 / (1.0 + (d / ev.config.min_distance).ln())
            }
        }
    }
}

fn parse_f32(value: &str, default: f32) -> f32 {
    value.trim().parse().unwrap_or(default)
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
; track ambience\n\
[crowd]\n\
type = ambient\n\
trigger_variable = RAIN\n\
trigger_condition = gt\n\
trigger_value = 0.3\n\
loop = 1\n\
camera_interior = 0.4\n\
camera_exterior = 0.9\n\
camera_track = 1.0\n\
attenuation = linear\n\
min_distance = 2\n\
max_distance = 100\n\
\n\
[pit_air]\n\
type = one_shot\n\
trigger_variable = WIND\n\
trigger_condition = lt\n\
trigger_value = 0.5\n\
";

    #[test]
    fn loads_events_with_fields() {
        let mut m = TrackAudioManager::new();
        assert!(m.load_config(SAMPLE));
        assert_eq!(m.events().len(), 2);

        let crowd = m.event("crowd").unwrap();
        assert_eq!(crowd.config.kind, "ambient");
        assert_eq!(crowd.config.trigger_variable, "RAIN");
        assert_eq!(crowd.config.trigger_condition, "gt");
        assert!((crowd.config.trigger_value - 0.3).abs() < 1e-6);
        assert!(crowd.config.loop_event);
        assert_eq!(crowd.config.attenuation, AttenuationModel::Linear);
        assert!((crowd.config.camera_interior_mult - 0.4).abs() < 1e-6);
        assert!((crowd.config.max_distance - 100.0).abs() < 1e-6);

        let pit = m.event("pit_air").unwrap();
        assert!(!pit.config.loop_event);
        assert_eq!(pit.config.attenuation, AttenuationModel::Inverse);
    }

    #[test]
    fn empty_or_keyless_text_yields_no_events() {
        let mut m = TrackAudioManager::new();
        assert!(!m.load_config(""));
        assert!(m.events().is_empty());
    }

    #[test]
    fn derive_trigger_value_maps_weather() {
        let w = WeatherState {
            cloud_cover: 0.7,
            rain_intensity: 0.2,
            wind_speed: 45.0,
            ..Default::default()
        };
        let d = TrackAudioManager::derive_trigger_value;
        assert!((d("SUN", &w) - 0.7).abs() < 1e-6);
        assert!((d("rain", &w) - 0.2).abs() < 1e-6);
        assert_eq!(d("WIND", &w), 1.0, "wind is capped");
        assert_eq!(d("BOGUS", &w), 0.0);
    }

    #[test]
    fn update_triggers_and_stops_by_condition() {
        let mut m = TrackAudioManager::new();
        m.load_config(SAMPLE);
        // Rain above threshold -> crowd playing.
        let wet = WeatherState {
            rain_intensity: 0.8,
            ..Default::default()
        };
        m.update(0.1, &wet, CameraMode::Chase);
        assert!(m.event("crowd").unwrap().is_playing);

        let dry = WeatherState::default();
        m.update(0.1, &dry, CameraMode::Chase);
        assert!(!m.event("crowd").unwrap().is_playing, "rain <= 0.3 stops");
    }

    #[test]
    fn empty_trigger_is_always_active() {
        let mut m = TrackAudioManager::new();
        m.load_config("[always]\ntype = ambient\n");
        m.update(0.1, &WeatherState::default(), CameraMode::Cockpit);
        assert!(m.event("always").unwrap().is_playing);
    }

    #[test]
    fn loop_playback_position_wraps() {
        let mut m = TrackAudioManager::new();
        m.load_config("[crowd]\ntrigger_variable =\nloop = 1\n");
        for _ in 0..250 {
            m.update(0.1, &WeatherState::default(), CameraMode::Chase);
        }
        let pos = m.event("crowd").unwrap().playback_position;
        assert!(
            (0.0..1.0).contains(&pos),
            "position wraps into [0,1): {pos}"
        );
    }

    #[test]
    fn last_trigger_value_is_recorded() {
        let mut m = TrackAudioManager::new();
        m.load_config(
            "[crowd]\ntrigger_variable = SUN\ntrigger_condition = gt\ntrigger_value = 0.0\n",
        );
        let w = WeatherState {
            cloud_cover: 0.42,
            ..Default::default()
        };
        m.update(0.1, &w, CameraMode::Tv);
        assert!((m.event("crowd").unwrap().last_trigger_value - 0.42).abs() < 1e-6);
    }

    #[test]
    fn camera_multipliers_switch_per_mode() {
        let mut m = TrackAudioManager::new();
        m.load_config("[e]\ncamera_interior = 0.4\ncamera_exterior = 0.9\ncamera_track = 1.0\n");
        let ev = m.event("e").unwrap();
        assert_eq!(ev.camera_multiplier(CameraMode::Cockpit), 0.4);
        assert_eq!(ev.camera_multiplier(CameraMode::Chase), 0.9);
        assert_eq!(ev.camera_multiplier(CameraMode::Free), 0.9);
        assert_eq!(ev.camera_multiplier(CameraMode::Tv), 1.0);
        assert_eq!(ev.camera_multiplier(CameraMode::Replay), 1.0);
    }

    #[test]
    fn attenuation_models_differ() {
        let mut m = TrackAudioManager::new();
        m.load_config(
            "[inv]\nattenuation = inverse\nmin_distance = 10\nmax_distance = 100\n\
             [lin]\nattenuation = linear\nmin_distance = 10\nmax_distance = 100\n\
             [log]\nattenuation = logarithmic\nmin_distance = 10\nmax_distance = 100\n",
        );
        assert_eq!(m.attenuation_gain("inv", 5.0), 1.0);
        assert_eq!(m.attenuation_gain("inv", 200.0), 0.0);
        assert_eq!(m.attenuation_gain("lin", 55.0), 0.5);
        let log_mid = m.attenuation_gain("log", 20.0);
        assert!(log_mid > 0.0 && log_mid < 1.0);
        assert_eq!(m.attenuation_gain("missing", 10.0), 0.0);
    }

    #[test]
    fn bool_parsing_variants() {
        assert!(parse_bool("1"));
        assert!(parse_bool(" TRUE "));
        assert!(parse_bool("on"));
        assert!(!parse_bool("0"));
        assert!(!parse_bool("off"));
    }

    #[test]
    fn missing_file_loads_nothing() {
        let missing = std::env::temp_dir().join("racingedge_track_audio_absent.ini");
        let mut m = TrackAudioManager::new();
        assert!(!m.load_config_file(&missing));
        assert!(m.events().is_empty());
    }
}
