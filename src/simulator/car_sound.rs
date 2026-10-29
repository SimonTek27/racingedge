//! Audio veicolo — architettura allineata a **ksengine `SimulatorAudio`**.
//!
//! Riferimento: https://github.com/SimonTek27/ksengine
//!   - `src/simulator/SimulatorAudio.h`  (layer RPM, categorie, updatePhysics)
//!   - `docs/audio.md`                   (KSBankParser FMOD 1.x/2.x senza SDK)
//!   - `docs/VEHICLE_AUDIO_HOOK.md`       (loadCarAudio / sound pack)
//!
//! Strategia (come ksengine, non come FMOD Studio API):
//! 1. Carica metadata eventi auto (`event:/cars/<stem>/…`)
//! 2. Se il `.bank` è presente → tenta estrazione sample / usa WAV in sfx/
//! 3. Altrimenti `generate_synth_engine_layer` (come `generateSynthEngineLayer`)
//! 4. Crossfade 8 layer int/ext su RPM + throttle
//! 5. Parametri `rpms` / `throttle` / `boost` come in CSP
//!
//! Bank di riferimento: `vrc_pt_2024_ferrenzo_csp.bank`

use bevy::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

use crate::simulator::audio_host::{update_car_mix, CarMix};
#[cfg(feature = "game-audio")]
use crate::simulator::audio_playback::CarAudioPlaybackPlugin;
use crate::simulator::physics::{VehicleConfig, VehicleState};
use crate::simulator::rebank::{ensure_default_rebank, load_rebank, RebankBuilder};

// =============================================================================
// CATEGORIE (ks::sim::SimulatorAudio::SoundCategory)
// =============================================================================

/// Eventi **specifici della macchina** (bank auto / `.rebank` car).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum SoundCategory {
    EngineInterior = 0,
    EngineExterior,
    Turbo,
    Wastegate,
    Blowoff,
    Wind,
    Transmission,
    TransmissionExt,
    GearShift,
    GearClonk,
    Brakes,
    Bodywork,
    Backfire,
    Limiter,
    Starter,
    Ignition,
    /// Sentinel: numero categorie auto
    Count,
}

impl SoundCategory {
    pub const CAR: [SoundCategory; 16] = [
        Self::EngineInterior,
        Self::EngineExterior,
        Self::Turbo,
        Self::Wastegate,
        Self::Blowoff,
        Self::Wind,
        Self::Transmission,
        Self::TransmissionExt,
        Self::GearShift,
        Self::GearClonk,
        Self::Brakes,
        Self::Bodywork,
        Self::Backfire,
        Self::Limiter,
        Self::Starter,
        Self::Ignition,
    ];

    pub const ALL: [SoundCategory; 16] = Self::CAR;

    pub fn is_car_specific(self) -> bool {
        !matches!(self, Self::Count)
    }
}

/// Eventi **common** (pista / meteo / superfici) — non nel bank macchina.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(usize)]
pub enum CommonSoundCategory {
    // --- meteo ---
    RainAmbient = 0,
    RainCar,
    RainThunder,
    // --- rolling / contatto continuo per superficie ---
    SurfaceAsphalt,
    SurfaceGrass,
    SurfaceGravel,
    SurfaceDirt,
    SurfaceSand,
    SurfaceMud,
    SurfaceKerb,
    SurfaceConcrete,
    SurfaceMetal,
    SurfaceWood,
    SurfaceIce,
    SurfaceSnow,
    SurfaceWet,
    SurfaceOil,
    SurfaceRumble,
    SurfaceCarpet,
    // --- skid (perdita aderenza) per superficie ---
    SkidAsphalt,
    SkidGrass,
    SkidGravel,
    SkidDirt,
    SkidSand,
    SkidMud,
    SkidKerb,
    SkidConcrete,
    SkidMetal,
    SkidIce,
    SkidSnow,
    SkidWet,
    Count,
}

impl CommonSoundCategory {
    pub const WEATHER: [CommonSoundCategory; 3] =
        [Self::RainAmbient, Self::RainCar, Self::RainThunder];

    /// Tutte le superfici di rolling (contatto continuo).
    pub const fn surface_rolling(i: usize) -> Option<CommonSoundCategory> {
        match i {
            0 => Some(Self::SurfaceAsphalt),
            1 => Some(Self::SurfaceGrass),
            2 => Some(Self::SurfaceGravel),
            3 => Some(Self::SurfaceDirt),
            4 => Some(Self::SurfaceSand),
            5 => Some(Self::SurfaceMud),
            6 => Some(Self::SurfaceKerb),
            7 => Some(Self::SurfaceConcrete),
            8 => Some(Self::SurfaceMetal),
            9 => Some(Self::SurfaceWood),
            10 => Some(Self::SurfaceIce),
            11 => Some(Self::SurfaceSnow),
            12 => Some(Self::SurfaceWet),
            13 => Some(Self::SurfaceOil),
            14 => Some(Self::SurfaceRumble),
            15 => Some(Self::SurfaceCarpet),
            _ => None,
        }
    }

    pub const SURFACE_ROLLING_COUNT: usize = 16;

    pub const SURFACE_SKID_COUNT: usize = 12;

    pub const fn surface_skid(i: usize) -> Option<CommonSoundCategory> {
        match i {
            0 => Some(Self::SkidAsphalt),
            1 => Some(Self::SkidGrass),
            2 => Some(Self::SkidGravel),
            3 => Some(Self::SkidDirt),
            4 => Some(Self::SkidSand),
            5 => Some(Self::SkidMud),
            6 => Some(Self::SkidKerb),
            7 => Some(Self::SkidConcrete),
            8 => Some(Self::SkidMetal),
            9 => Some(Self::SkidIce),
            10 => Some(Self::SkidSnow),
            11 => Some(Self::SkidWet),
            _ => None,
        }
    }
}

/// Tipo superficie pista (per scegliere rolling/skid common).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrackSurface {
    #[default]
    Asphalt,
    Grass,
    Gravel,
    Dirt,
    Sand,
    Mud,
    Kerb,
    Concrete,
    Metal,
    Wood,
    Ice,
    Snow,
    Wet,
    Oil,
    Rumble,
    Carpet,
}

impl TrackSurface {
    pub fn rolling(self) -> CommonSoundCategory {
        match self {
            Self::Asphalt => CommonSoundCategory::SurfaceAsphalt,
            Self::Grass => CommonSoundCategory::SurfaceGrass,
            Self::Gravel => CommonSoundCategory::SurfaceGravel,
            Self::Dirt => CommonSoundCategory::SurfaceDirt,
            Self::Sand => CommonSoundCategory::SurfaceSand,
            Self::Mud => CommonSoundCategory::SurfaceMud,
            Self::Kerb => CommonSoundCategory::SurfaceKerb,
            Self::Concrete => CommonSoundCategory::SurfaceConcrete,
            Self::Metal => CommonSoundCategory::SurfaceMetal,
            Self::Wood => CommonSoundCategory::SurfaceWood,
            Self::Ice => CommonSoundCategory::SurfaceIce,
            Self::Snow => CommonSoundCategory::SurfaceSnow,
            Self::Wet => CommonSoundCategory::SurfaceWet,
            Self::Oil => CommonSoundCategory::SurfaceOil,
            Self::Rumble => CommonSoundCategory::SurfaceRumble,
            Self::Carpet => CommonSoundCategory::SurfaceCarpet,
        }
    }

    pub fn skid(self) -> CommonSoundCategory {
        match self {
            Self::Asphalt | Self::Rumble | Self::Carpet => CommonSoundCategory::SkidAsphalt,
            Self::Grass => CommonSoundCategory::SkidGrass,
            Self::Gravel => CommonSoundCategory::SkidGravel,
            Self::Dirt => CommonSoundCategory::SkidDirt,
            Self::Sand => CommonSoundCategory::SkidSand,
            Self::Mud => CommonSoundCategory::SkidMud,
            Self::Kerb => CommonSoundCategory::SkidKerb,
            Self::Concrete => CommonSoundCategory::SkidConcrete,
            Self::Metal | Self::Wood => CommonSoundCategory::SkidMetal,
            Self::Ice => CommonSoundCategory::SkidIce,
            Self::Snow => CommonSoundCategory::SkidSnow,
            Self::Wet | Self::Oil => CommonSoundCategory::SkidWet,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraMode {
    Cockpit,
    Chase,
    Free,
}

// =============================================================================
// LAYER MOTORE (EngineLayer in SimulatorAudio)
// =============================================================================

const ENGINE_LAYERS: usize = 8;

#[derive(Clone, Debug, Default)]
pub struct EngineLayer {
    /// PCM interleaved f32 (mono o stereo)
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub rpm_min: f32,
    pub rpm_max: f32,
    pub volume: f32,
    pub looped: bool,
}

impl EngineLayer {
    /// Sintesi procedurale (equivalente generateSynthEngineLayer di ksengine).
    pub fn synthesize(rpm_min: f32, rpm_max: f32, sample_rate: u32, duration_s: f32) -> Self {
        let n = (sample_rate as f32 * duration_s) as usize;
        let mid_rpm = (rpm_min + rpm_max) * 0.5;
        // fondamentale ~ (rpm/60)*(cylinders/2) → usiamo 3 “firing” eq. per realisticità
        let base_hz = (mid_rpm / 60.0) * 1.5;
        let mut samples = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / sample_rate as f32;
            // armoniche decrescenti + rumore leggero
            let mut s = 0.0;
            for h in 1..=6 {
                let amp = 1.0 / (h as f32).powf(1.2);
                s += amp * (2.0 * std::f32::consts::PI * base_hz * h as f32 * t).sin();
            }
            // noise modulato
            let noise = ((i.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as i16 as f32
                / 32768.0)
                * 0.08;
            s = (s * 0.22 + noise).clamp(-1.0, 1.0);
            samples.push(s);
        }
        Self {
            samples,
            sample_rate,
            channels: 1,
            rpm_min,
            rpm_max,
            volume: 1.0,
            looped: true,
        }
    }

    /// Peso di crossfade per un dato RPM (triangolo tra rpm_min e rpm_max).
    pub fn weight_at_rpm(&self, rpm: f32) -> f32 {
        if rpm < self.rpm_min || rpm > self.rpm_max {
            return 0.0;
        }
        let mid = (self.rpm_min + self.rpm_max) * 0.5;
        let half = (self.rpm_max - self.rpm_min) * 0.5;
        if half < 1.0 {
            return 1.0;
        }
        (1.0 - (rpm - mid).abs() / half).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Debug, Default)]
pub struct SfxSample {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub looped: bool,
    pub volume: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct PlaybackState {
    pub position: f32,
    pub playing: bool,
    pub volume: f32,
    pub pitch: f32,
}

// =============================================================================
// BANK META + PATH
// =============================================================================

#[derive(Clone, Debug)]
pub struct CarSoundBankMeta {
    pub bank_file: String,
    pub bank_stem: String,
    pub rpm_min: f32,
    pub rpm_max: f32,
    pub vol_engine_ext: f32,
    pub vol_engine_int: f32,
}

impl Default for CarSoundBankMeta {
    fn default() -> Self {
        Self {
            bank_file: "vrc_pt_2024_ferrenzo_csp.bank".into(),
            bank_stem: "vrc_pt_2024_ferrenzo_csp".into(),
            rpm_min: 1200.0,
            rpm_max: 8000.0,
            vol_engine_ext: 1.0,
            vol_engine_int: 0.85,
        }
    }
}

impl CarSoundBankMeta {
    pub fn load_auto() -> Self {
        for p in [
            "assets/sfx/vrc_pt_2024_ferrenzo_csp.json",
            "assets/sfx/car_sound.json",
        ] {
            if let Ok(t) = fs::read_to_string(p) {
                let mut m = Self::default();
                if let Some(s) = json_str(&t, "bank") {
                    m.bank_file = s;
                }
                if let Some(s) = json_str(&t, "bank_stem") {
                    m.bank_stem = s;
                }
                if let Some(v) = json_f32(&t, "rpm_min") {
                    m.rpm_min = v;
                }
                if let Some(v) = json_f32(&t, "rpm_max") {
                    m.rpm_max = v;
                }
                eprintln!("[car_sound] meta {p}");
                return m;
            }
        }
        Self::default()
    }

    pub fn event_path(&self, name: &str) -> String {
        format!("event:/cars/{}/{}", self.bank_stem, name)
    }

    pub fn bank_candidates(&self) -> Vec<PathBuf> {
        vec![
            PathBuf::from("assets/sfx").join(&self.bank_file),
            PathBuf::from("/home/workdir/attachments").join(&self.bank_file),
            PathBuf::from("/home/workdir/artifacts/racingedge/assets/sfx").join(&self.bank_file),
        ]
    }
}

fn json_str(text: &str, key: &str) -> Option<String> {
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

fn json_f32(text: &str, key: &str) -> Option<f32> {
    let pat = format!("\"{key}\"");
    let idx = text.find(&pat)?;
    let after = text[idx + pat.len()..]
        .trim_start()
        .trim_start_matches(':')
        .trim_start();
    after.split([',', '}', '\n']).next()?.trim().parse().ok()
}

// =============================================================================
// STATO (SimulatorAudio)
// =============================================================================

#[derive(Clone, Copy, Debug, Default)]
pub struct AudioPhysicsState {
    pub rpm: f32,
    pub throttle: f32,
    pub brake: f32,
    pub speed: f32,
    pub gear: i8,
    pub is_shifting: bool,
    pub slip_ratio: f32,
    pub slip_angle: f32,
    pub boost: f32,
}

/// Resource principale — analogo a `ks::sim::SimulatorAudio`.
#[derive(Resource)]
pub struct CarSound {
    pub meta: CarSoundBankMeta,
    pub enabled: bool,
    pub master: f32,
    pub engine_volume: f32,
    pub environment_volume: f32,
    pub interior_mix: f32,
    pub camera: CameraMode,
    pub bank_present: bool,
    pub using_synth: bool,

    pub engine_int: [EngineLayer; ENGINE_LAYERS],
    pub engine_ext: [EngineLayer; ENGINE_LAYERS],
    pub playback: [PlaybackState; SoundCategory::Count as usize],

    pub state: AudioPhysicsState,
    pub prev_gear: i8,
    pub gear_shift_timer: f32,
    pub backfire_timer: f32,
    pub limiter_timer: f32,

    /// Mix stereo ultimo blocco (debug / futuro rodio callback)
    pub last_mix_peak: f32,
}

impl Default for CarSound {
    fn default() -> Self {
        let meta = CarSoundBankMeta::load_auto();
        let bank_present = meta.bank_candidates().iter().any(|p| p.exists());
        let mut s = Self {
            meta: meta.clone(),
            enabled: true,
            master: 0.8,
            engine_volume: 1.0,
            environment_volume: 0.6,
            interior_mix: 0.85,
            camera: CameraMode::Chase,
            bank_present,
            using_synth: true,
            engine_int: std::array::from_fn(|_| EngineLayer::default()),
            engine_ext: std::array::from_fn(|_| EngineLayer::default()),
            playback: [PlaybackState::default(); SoundCategory::Count as usize],
            state: AudioPhysicsState::default(),
            prev_gear: 1,
            gear_shift_timer: 0.0,
            backfire_timer: 0.0,
            limiter_timer: 0.0,
            last_mix_peak: 0.0,
        };
        s.load_or_synthesize_layers();
        if bank_present {
            eprintln!(
                "[car_sound] bank trovato ({}) — layer synth pronti; \
                 estrazione FSB completa richiede KSBankParser (ksengine)",
                meta.bank_file
            );
        } else {
            eprintln!("[car_sound] bank assente → solo sintesi (come generateSynthEngineLayer)");
        }
        s
    }
}

impl CarSound {
    /// Come `loadCarAudio(carDirectory)`: cerca bank + eventuale sfx/*.wav.
    pub fn load_car_audio(&mut self, car_or_sfx_dir: impl AsRef<Path>) {
        let dir = car_or_sfx_dir.as_ref();
        // bank nella cartella?
        for e in fs::read_dir(dir).into_iter().flatten().flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("bank") {
                self.meta.bank_file = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&self.meta.bank_file)
                    .to_string();
                if let Some(stem) = p.file_stem().and_then(|s| s.to_str()) {
                    self.meta.bank_stem = stem.to_string();
                }
                self.bank_present = true;
                eprintln!("[car_sound] loadCarAudio bank={}", p.display());
            }
        }
        self.load_or_synthesize_layers();
    }

    fn load_or_synthesize_layers(&mut self) {
        // 1) .rebank nativo (Bevy-friendly)
        let rebank_path = ensure_default_rebank();
        if let Ok((manifest, ext, int_layers)) = load_rebank(&rebank_path) {
            self.meta.rpm_min = manifest.rpm_min;
            self.meta.rpm_max = manifest.rpm_max;
            for (i, layer) in ext.into_iter().take(ENGINE_LAYERS).enumerate() {
                self.engine_ext[i] = layer;
            }
            for (i, layer) in int_layers.into_iter().take(ENGINE_LAYERS).enumerate() {
                self.engine_int[i] = layer;
            }
            self.using_synth = false;
            self.bank_present = true;
            eprintln!(
                "[car_sound] loaded .rebank {} ({} events)",
                rebank_path.display(),
                manifest.events.len()
            );
            return;
        }
        // 2) fallback sintesi in-memory
        let lo = self.meta.rpm_min;
        let hi = self.meta.rpm_max.max(lo + 1000.0);
        let step = (hi - lo) / ENGINE_LAYERS as f32;
        for i in 0..ENGINE_LAYERS {
            let a = lo + i as f32 * step;
            let b = a + step * 1.35;
            self.engine_int[i] = EngineLayer::synthesize(a, b, 44100, 0.5);
            self.engine_int[i].volume = self.meta.vol_engine_int;
            self.engine_ext[i] = EngineLayer::synthesize(a, b.min(hi * 1.05), 44100, 0.5);
            self.engine_ext[i].volume = self.meta.vol_engine_ext;
        }
        self.using_synth = true;
    }

    /// Rigenera il pack `.rebank` su disco (tool / editor).
    pub fn rebuild_rebank(
        &mut self,
        parent: impl AsRef<std::path::Path>,
    ) -> Result<std::path::PathBuf, String> {
        let path = RebankBuilder::new(&self.meta.bank_stem, parent)
            .with_rpm_range(self.meta.rpm_min, self.meta.rpm_max)
            .build_from_synth()?;
        self.load_or_synthesize_layers();
        Ok(path)
    }

    /// `updatePhysics` (ksengine).
    pub fn update_physics(&mut self, p: AudioPhysicsState, dt: f32) {
        if p.gear != self.prev_gear {
            self.gear_shift_timer = 0.12;
            self.prev_gear = p.gear;
        }
        self.gear_shift_timer = (self.gear_shift_timer - dt).max(0.0);

        if p.throttle < 0.05 && self.state.throttle > 0.4 && p.rpm > 4500.0 {
            self.backfire_timer = 0.1;
        }
        self.backfire_timer = (self.backfire_timer - dt).max(0.0);

        if p.rpm >= self.meta.rpm_max * 0.98 {
            self.limiter_timer = 0.05;
        }
        self.limiter_timer = (self.limiter_timer - dt).max(0.0);

        self.state = p;

        // avanza playhead layer
        for i in 0..ENGINE_LAYERS {
            for set in [&mut self.engine_int[i], &mut self.engine_ext[i]] {
                if set.samples.is_empty() {
                    continue;
                }
                let len = set.samples.len() as f32 / set.channels as f32;
                let speed = (p.rpm / ((set.rpm_min + set.rpm_max) * 0.5).max(1.0)).clamp(0.5, 2.0);
                // position stored in playback engine channels
                let _ = (len, speed);
            }
        }
        let engine_gain = self.engine_gain();
        let ext = &mut self.playback[SoundCategory::EngineExterior as usize];
        ext.playing = true;
        ext.pitch = (p.rpm / 4000.0).clamp(0.4, 2.2);
        ext.volume = engine_gain;

        let wind = &mut self.playback[SoundCategory::Wind as usize];
        wind.playing = p.speed > 5.0;
        wind.volume = ((p.speed / 80.0).clamp(0.0, 1.0)).powf(1.4) * self.environment_volume;

        self.playback[SoundCategory::Backfire as usize].volume =
            if self.backfire_timer > 0.0 { 0.9 } else { 0.0 };
        self.playback[SoundCategory::Limiter as usize].volume =
            if self.limiter_timer > 0.0 { 0.7 } else { 0.0 };
        self.playback[SoundCategory::GearShift as usize].volume = if self.gear_shift_timer > 0.0 {
            0.8
        } else {
            0.0
        };

        self.last_mix_peak = self.estimate_peak();
    }

    /// Engine mix gain for the current rpm, load, camera and volumes.
    pub fn engine_gain(&self) -> f32 {
        let load = self.state.throttle.max(0.08);
        let rpm_n = ((self.state.rpm - self.meta.rpm_min)
            / (self.meta.rpm_max - self.meta.rpm_min).max(1.0))
        .clamp(0.0, 1.2);
        let cam = match self.camera {
            CameraMode::Cockpit => self.interior_mix,
            _ => 1.0,
        };
        (0.2 + 0.8 * load * (0.35 + 0.65 * rpm_n)) * self.engine_volume * self.master * cam
    }

    /// Mix level stimato (somma pesi layer) — utile finché non c’è device callback.
    pub fn estimate_peak(&self) -> f32 {
        let rpm = self.state.rpm;
        let mut w = 0.0;
        for layer in &self.engine_ext {
            w += layer.weight_at_rpm(rpm) * layer.volume;
        }
        (w * self.engine_gain()).clamp(0.0, 1.0)
    }

    /// Parametri FMOD-style per eventuale binding futuro / debug.
    pub fn fmod_params(&self) -> Vec<(&'static str, f32)> {
        vec![
            ("rpms", self.state.rpm),
            ("throttle", self.state.throttle),
            ("state", if self.state.gear == 0 { 0.0 } else { 1.0 }),
            ("boost", self.state.boost),
            ("speed", self.state.speed * 3.6),
        ]
    }

    /// Standard engine event path (like KsCarAudioEngine::eventPath).
    pub fn engine_ext_event(&self) -> String {
        self.meta.event_path("engine_ext")
    }
}

// =============================================================================
// BEVY SYSTEMS
// =============================================================================

pub fn update_car_sound(
    time: Res<Time>,
    cfg: Res<VehicleConfig>,
    mut sound: ResMut<CarSound>,
    q: Query<&VehicleState>,
) {
    if !sound.enabled {
        return;
    }
    let Ok(car) = q.single() else {
        return;
    };
    let p = AudioPhysicsState {
        rpm: car
            .engine
            .rpm
            .clamp(sound.meta.rpm_min * 0.5, sound.meta.rpm_max * 1.05),
        throttle: car.throttle,
        brake: car.brake,
        speed: car.speed.abs(),
        gear: car.drivetrain.gear,
        is_shifting: false,
        slip_ratio: 0.0,
        slip_angle: car.lat_vel.abs() * 0.05,
        boost: (car.throttle * (car.engine.rpm / cfg.engine.rpm_limiter.max(1.0))).clamp(0.0, 1.0),
    };
    sound.update_physics(p, time.delta_secs());
}

pub fn debug_car_sound(time: Res<Time>, sound: Res<CarSound>) {
    if time.elapsed_secs().fract() > 0.05 {
        return;
    }
    if !(time.elapsed_secs() as u32).is_multiple_of(2) {
        return;
    }
    eprintln!(
        "[car_sound] {} rpm={:.0} thr={:.0}% peak={:.2} mode={} event={}",
        sound.meta.bank_stem,
        sound.state.rpm,
        sound.state.throttle * 100.0,
        sound.last_mix_peak,
        if sound.using_synth {
            "synth-layers"
        } else {
            "bank-pcm"
        },
        sound.engine_ext_event()
    );
}

// =============================================================================
// COMMON AUDIO (pista / meteo / superfici) — fuori dal .rebank macchina
// =============================================================================

#[derive(Resource)]
pub struct CommonSound {
    pub enabled: bool,
    pub rain_intensity: f32,
    pub wetness: f32,
    pub active_surface: TrackSurface,
    pub volumes: [f32; CommonSoundCategory::Count as usize],
}

impl Default for CommonSound {
    fn default() -> Self {
        Self {
            enabled: true,
            rain_intensity: 0.0,
            wetness: 0.0,
            active_surface: TrackSurface::Asphalt,
            volumes: [0.0; CommonSoundCategory::Count as usize],
        }
    }
}

impl CommonSound {
    pub fn set_weather(&mut self, rain: f32, wetness: f32) {
        self.rain_intensity = rain.clamp(0.0, 1.0);
        self.wetness = wetness.clamp(0.0, 1.0);
        self.volumes[CommonSoundCategory::RainAmbient as usize] = self.rain_intensity * 0.6;
        self.volumes[CommonSoundCategory::RainCar as usize] = self.rain_intensity * 0.4;
        self.volumes[CommonSoundCategory::RainThunder as usize] =
            if self.rain_intensity > 0.7 { 0.3 } else { 0.0 };
    }

    pub fn set_surface(&mut self, surface: TrackSurface, speed_ms: f32, slip: f32) {
        self.active_surface = surface;
        // azzera rolling/skid
        for i in 0..CommonSoundCategory::SURFACE_ROLLING_COUNT {
            if let Some(c) = CommonSoundCategory::surface_rolling(i) {
                self.volumes[c as usize] = 0.0;
            }
        }
        for i in 0..CommonSoundCategory::SURFACE_SKID_COUNT {
            if let Some(c) = CommonSoundCategory::surface_skid(i) {
                self.volumes[c as usize] = 0.0;
            }
        }
        let roll = surface.rolling() as usize;
        let sk = surface.skid() as usize;
        let roll_vol = ((speed_ms / 40.0).clamp(0.0, 1.0)).powf(1.2) * 0.35;
        let skid_vol = slip.clamp(0.0, 1.0) * 0.8;
        if roll < self.volumes.len() {
            self.volumes[roll] = roll_vol;
        }
        if sk < self.volumes.len() {
            self.volumes[sk] = skid_vol;
        }
    }
}

pub struct CarSoundPlugin;

impl Plugin for CarSoundPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CarSound>()
            .init_resource::<CommonSound>()
            // The mix frame is published here, not in the playback plugin: a
            // host with its own mixer needs it and never builds `bevy_audio`.
            .init_resource::<CarMix>()
            .add_systems(Update, (update_car_sound, update_car_mix).chain())
            .add_systems(Update, debug_car_sound);
        // The voices need `bevy_audio`; a host that brings its own mixer builds
        // without the `game-audio` feature and keeps the mix models only.
        #[cfg(feature = "game-audio")]
        app.add_plugins(CarAudioPlaybackPlugin);
    }
}

/// Alias retrocompatibile.
pub type CarSoundState = CarSound;
pub type CarSoundPluginCompat = CarSoundPlugin;
