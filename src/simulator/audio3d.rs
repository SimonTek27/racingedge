//! Audio 3D (doppler, attenuazione, pan) e volumi/pitch per evento.
//!
//! Port di `src/simulator/Audio3D.h` e `src/simulator/CarEventVolumes.h` di
//! ksengine. Sono regole pure, tenute fuori dal mixer così restano testabili.

use std::collections::HashMap;

use bevy::math::Vec3;

use super::car_sound::{CommonSoundCategory, SoundCategory};

/// Velocità del suono a ~15 °C (m/s).
pub const K_SPEED_OF_SOUND: f32 = 343.0;
/// Fattore doppler minimo.
pub const K_DOPPLER_MIN: f32 = 0.70;
/// Fattore doppler massimo.
pub const K_DOPPLER_MAX: f32 = 1.45;

/// Un'auto udibile: dove si trova, come si muove e a quanti giri gira.
#[derive(Clone, Copy, Debug, Default)]
pub struct OtherCarVoice {
    /// Posizione nel mondo (m).
    pub pos: Vec3,
    /// Velocità nel mondo (m/s).
    pub vel: Vec3,
    /// Giri motore per il tono.
    pub rpm: f32,
    /// Livello della sorgente.
    pub gain: f32,
}

/// Fattore di pitch dalla velocità di avvicinamento (m/s, > 0 = in
/// avvicinamento), con clamp a un range musicale.
pub fn doppler_factor(closing_speed: f32, speed_of_sound: f32) -> f32 {
    let c = if speed_of_sound > 1.0 {
        speed_of_sound
    } else {
        1.0
    };
    (1.0 + closing_speed / c).clamp(K_DOPPLER_MIN, K_DOPPLER_MAX)
}

/// Guadagno per distanza: 1 fino a `ref_dist`, poi `ref_dist/d`, con fade a 0
/// sull'ultimo 20% di `max_dist`.
pub fn distance_gain(distance: f32, ref_dist: f32, max_dist: f32) -> f32 {
    if distance <= ref_dist {
        return 1.0;
    }
    if distance >= max_dist {
        return 0.0;
    }
    let fade = ((max_dist - distance) / (max_dist * 0.2)).clamp(0.0, 1.0);
    (ref_dist / distance) * fade
}

/// Pan equal-power per una sorgente rispetto all'asse frontale dell'ascoltatore:
/// +1 = tutto a destra. `right = forward × up` (destrorso).
pub fn bearing_pan(to: Vec3, forward: Vec3, up: Vec3) -> f32 {
    let len = to.length();
    if len < 1e-6 {
        return 0.0;
    }
    let i = to / len;

    let right = forward.cross(up);
    let rl = right.length();
    if rl < 1e-6 {
        return 0.0;
    }
    let r = right / rl;

    i.dot(r).clamp(-1.0, 1.0)
}

/// Volumi e pitch per evento di un'auto (port di `CarEventVolumes`), secondo il
/// formato CSP `[AUDIO_VOLUME]` / `[AUDIO_PITCH]`.
#[derive(Clone, Debug, Default)]
pub struct CarEventVolumes {
    /// Moltiplicatori di volume per chiave (maiuscola).
    pub volume: HashMap<String, f32>,
    /// Moltiplicatori di pitch per chiave (maiuscola).
    pub pitch: HashMap<String, f32>,
    /// Almeno una chiave è stata letta.
    pub loaded: bool,
}

impl CarEventVolumes {
    /// Volume per chiave, con default.
    pub fn vol(&self, key: &str, default: f32) -> f32 {
        self.volume.get(key).copied().unwrap_or(default)
    }

    /// Pitch per chiave, con default.
    pub fn pit(&self, key: &str, default: f32) -> f32 {
        self.pitch.get(key).copied().unwrap_or(default)
    }

    /// Volume mappato su una categoria dell'auto (port di `categoryVolume`).
    pub fn category_volume(&self, cat: SoundCategory) -> f32 {
        use SoundCategory as C;
        match cat {
            C::EngineInterior => self.vol("ENGINE_INT", 1.0),
            C::EngineExterior => self.vol("ENGINE_EXT", 1.0),
            C::Turbo | C::Wastegate | C::Blowoff => self.vol("TURBO", 1.0),
            C::Wind => self.vol("WIND", 1.0),
            C::Transmission | C::TransmissionExt => self.vol("TRANSMISSION", 1.0),
            C::GearShift => self.vol("GEAR_EXT", self.vol("GEAR_INT", 1.0)),
            C::GearClonk => self.vol("GEAR_INT", self.vol("GEAR_GRIND", 1.0)),
            C::Brakes | C::Bodywork => self.vol("BODYWORK", 1.0),
            C::Backfire => self.vol("BACKFIRE_EXT", self.vol("BACKFIRE_INT", 1.0)),
            C::Limiter => self.vol("LIMITER", 1.0),
            C::Starter | C::Ignition => self.vol("ENGINE_INT", 1.0),
            C::Count => 1.0,
        }
    }

    /// Pitch mappato su una categoria dell'auto (port di `categoryPitch`).
    pub fn category_pitch(&self, cat: SoundCategory) -> f32 {
        use SoundCategory as C;
        match cat {
            C::EngineInterior => self.pit("ENGINE_INT", 1.0),
            C::EngineExterior => self.pit("ENGINE_EXT", 1.0),
            C::Turbo => self.pit("TURBO", 1.0),
            C::Wind => self.pit("WIND", 1.0),
            C::Transmission => self.pit("TRANSMISSION", 1.0),
            C::GearShift => self.pit("GEAR_EXT", self.pit("GEAR_INT", 1.0)),
            C::GearClonk => self.pit("GEAR_INT", 1.0),
            C::Bodywork => self.pit("BODYWORK", 1.0),
            C::Backfire => self.pit("BACKFIRE_EXT", 1.0),
            C::Limiter => self.pit("LIMITER", 1.0),
            _ => 1.0,
        }
    }

    /// Volume per una categoria comune (skid), con i fallback dell'originale.
    pub fn common_volume(&self, cat: CommonSoundCategory) -> f32 {
        use CommonSoundCategory as C;
        match cat {
            C::SkidAsphalt | C::SkidKerb => self.vol("SKID_EXT", self.vol("DIRT", 1.0)),
            C::SkidGrass | C::SkidGravel => self.vol("DIRT", 1.0),
            C::SkidWet => self.vol("SKID_EXT", 1.0),
            _ => 1.0,
        }
    }

    /// Carica il file; un file assente restituisce i valori predefiniti
    /// (port di `loadFile`).
    pub fn load_file(path: &std::path::Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(_) => Self::default(),
        }
    }

    /// Cerca `audio_volumes.ini` nella cartella auto (port di `loadForCar`).
    pub fn load_for_car(car_directory: &std::path::Path) -> Self {
        const CANDIDATES: [&str; 4] = [
            "extension/audio_volumes.ini",
            "extension/ext_config.ini",
            "data/audio_volumes.ini",
            "audio_volumes.ini",
        ];
        for rel in CANDIDATES {
            let v = Self::load_file(&car_directory.join(rel));
            if v.loaded {
                return v;
            }
        }
        Self::default()
    }

    /// Analizza il testo (port del corpo di `loadFile`).
    pub fn parse(text: &str) -> Self {
        let mut out = Self::default();
        let mut section = String::new();

        for raw in text.lines() {
            let line = match raw.split_once(';') {
                Some((before, _)) => before,
                None => raw,
            }
            .trim()
            .to_string();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].trim().to_uppercase();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim().to_uppercase();
            let value: f32 = value.trim().parse().unwrap_or(0.0);
            if section == "AUDIO_VOLUME" || section == "VOLUME" {
                out.volume.insert(key, value.clamp(0.0, 4.0));
            } else if section == "AUDIO_PITCH" || section == "PITCH" {
                out.pitch.insert(key, value.clamp(0.25, 4.0));
            }
        }
        out.loaded = !out.volume.is_empty() || !out.pitch.is_empty();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doppler_raises_on_approach_and_lowers_on_recede() {
        let approaching = doppler_factor(100.0, K_SPEED_OF_SOUND);
        let receding = doppler_factor(-100.0, K_SPEED_OF_SOUND);
        assert!(approaching > 1.0);
        assert!(receding < 1.0);
        assert!((approaching - (1.0 + 100.0 / 343.0)).abs() < 1e-5);
    }

    #[test]
    fn doppler_is_clamped() {
        assert_eq!(doppler_factor(1e6, K_SPEED_OF_SOUND), K_DOPPLER_MAX);
        assert_eq!(doppler_factor(-1e6, K_SPEED_OF_SOUND), K_DOPPLER_MIN);
        // A degenerate speed of sound falls back to 1 m/s.
        assert_eq!(doppler_factor(0.0, 0.0), 1.0);
    }

    #[test]
    fn distance_gain_full_within_ref() {
        assert_eq!(distance_gain(0.0, 6.0, 300.0), 1.0);
        assert_eq!(distance_gain(6.0, 6.0, 300.0), 1.0);
    }

    #[test]
    fn distance_gain_decays_and_fades_to_zero() {
        let near = distance_gain(10.0, 6.0, 300.0);
        let mid = distance_gain(100.0, 6.0, 300.0);
        assert!(near > mid);
        assert_eq!(distance_gain(300.0, 6.0, 300.0), 0.0);
        assert_eq!(distance_gain(400.0, 6.0, 300.0), 0.0);
        // The fade starts inside the last 20% of maxDist.
        let fading = distance_gain(250.0, 6.0, 300.0);
        let inner = distance_gain(239.0, 6.0, 300.0);
        assert!(fading < inner);
    }

    #[test]
    fn bearing_pan_left_centre_right() {
        let fwd = Vec3::Z;
        let up = Vec3::Y;
        // right = forward × up = -X.
        assert!((bearing_pan(-Vec3::X, fwd, up) - 1.0).abs() < 1e-6);
        assert!((bearing_pan(Vec3::X, fwd, up) + 1.0).abs() < 1e-6);
        assert_eq!(bearing_pan(fwd, fwd, up), 0.0);
    }

    #[test]
    fn bearing_pan_handles_degenerate_inputs() {
        assert_eq!(bearing_pan(Vec3::ZERO, Vec3::Z, Vec3::Y), 0.0);
        // forward parallel to up -> no valid right vector.
        assert_eq!(bearing_pan(Vec3::X, Vec3::Y, Vec3::Y), 0.0);
    }

    #[test]
    fn parse_reads_sections_and_clamps() {
        let text = "# comment\n\
                    [AUDIO_VOLUME]\n\
                    ENGINE_EXT = 1.2 ; inline\n\
                    turbo=9.0\n\
                    [AUDIO_PITCH]\n\
                    ENGINE_EXT = 0.1\n\
                    ENGINE_INT = 1.1\n";
        let v = CarEventVolumes::parse(text);
        assert!(v.loaded);
        assert!((v.vol("ENGINE_EXT", 1.0) - 1.2).abs() < 1e-6);
        assert_eq!(v.vol("TURBO", 1.0), 4.0, "volume clamps to 4");
        assert_eq!(v.pit("ENGINE_EXT", 1.0), 0.25, "pitch clamps to 0.25");
        assert!((v.pit("ENGINE_INT", 1.0) - 1.1).abs() < 1e-6);
    }

    #[test]
    fn parse_without_section_is_ignored() {
        let v = CarEventVolumes::parse("ENGINE_EXT = 2.0\n");
        assert!(!v.loaded);
        assert_eq!(v.vol("ENGINE_EXT", 1.0), 1.0);
    }

    #[test]
    fn category_volume_maps_and_falls_back() {
        let mut v = CarEventVolumes::default();
        v.volume.insert("ENGINE_INT".into(), 1.5);
        v.volume.insert("GEAR_INT".into(), 0.8);
        v.volume.insert("WIND".into(), 0.5);
        assert_eq!(v.category_volume(SoundCategory::EngineInterior), 1.5);
        assert_eq!(v.category_volume(SoundCategory::Starter), 1.5);
        // GEAR_EXT missing -> GEAR_INT fallback.
        assert_eq!(v.category_volume(SoundCategory::GearShift), 0.8);
        assert_eq!(v.category_volume(SoundCategory::Wind), 0.5);
        // No key -> default 1.0.
        assert_eq!(v.category_volume(SoundCategory::Bodywork), 1.0);
    }

    #[test]
    fn category_pitch_defaults_to_one() {
        let mut v = CarEventVolumes::default();
        v.pitch.insert("ENGINE_EXT".into(), 1.2);
        assert_eq!(v.category_pitch(SoundCategory::EngineExterior), 1.2);
        assert_eq!(v.category_pitch(SoundCategory::EngineInterior), 1.0);
    }

    #[test]
    fn common_skid_volume_fallbacks() {
        let mut v = CarEventVolumes::default();
        v.volume.insert("DIRT".into(), 0.7);
        assert_eq!(v.common_volume(CommonSoundCategory::SkidGrass), 0.7);
        assert_eq!(v.common_volume(CommonSoundCategory::SkidAsphalt), 0.7);
        v.volume.insert("SKID_EXT".into(), 1.3);
        assert_eq!(v.common_volume(CommonSoundCategory::SkidWet), 1.3);
    }

    #[test]
    fn load_missing_file_is_empty() {
        let missing = std::env::temp_dir().join("racingedge_audio_absent.ini");
        let v = CarEventVolumes::load_file(&missing);
        assert!(!v.loaded);
    }

    #[test]
    fn other_car_voice_is_copy_and_default() {
        let voice = OtherCarVoice {
            pos: Vec3::new(1.0, 0.0, 2.0),
            vel: Vec3::new(0.0, 0.0, -5.0),
            rpm: 4000.0,
            gain: 0.8,
        };
        let copy = voice;
        assert_eq!(copy.rpm, 4000.0);
        assert_eq!(OtherCarVoice::default().gain, 0.0);
    }
}
