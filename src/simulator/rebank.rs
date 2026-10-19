//! RacingEdge Sound Bank (`.rebank`) — equivalente concettuale di un FMOD `.bank`
//! ma **indipendente da FMOD**, leggibile da Bevy senza SDK.
//!
//! Layout su disco:
//! ```text
//! assets/sfx/<name>.rebank/
//!   manifest.json          # eventi, layer RPM, parametri, volumi
//!   layers/
//!     engine_ext_00.wav
//!     engine_ext_01.wav
//!     ...
//!     engine_int_00.wav
//!   sfx/
//!     gear_shift.wav
//!     backfire.wav
//!     limiter.wav
//!     wind.wav
//! ```
//!
//! - **car** `.rebank` → solo eventi macchina (motore, turbo, gear, …)
//! - **common** `assets/sfx/common.rebank/` → rain, superfici pista, skid non-asfalto
//!
//! `manifest.json` è JSON plain; i sample sono WAV PCM 16-bit mono 44.1 kHz
//! (Bevy / rodio li aprono senza plugin).

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::simulator::car_sound::{EngineLayer, SoundCategory};

pub const REBANK_EXT: &str = "rebank";
pub const SAMPLE_RATE: u32 = 44100;
pub const ENGINE_LAYER_COUNT: usize = 8;

/// Descrizione di un layer motore nel manifest.
#[derive(Clone, Debug)]
pub struct LayerDesc {
    pub file: String,
    pub rpm_min: f32,
    pub rpm_max: f32,
    pub volume: f32,
    pub looped: bool,
}

/// Evento (come event:/cars/... ma path relativo al rebank).
#[derive(Clone, Debug)]
pub struct EventDesc {
    pub id: String,
    pub category: String,
    pub layers: Vec<LayerDesc>,
    pub oneshot: Option<String>,
}

/// Manifest completo.
#[derive(Clone, Debug)]
pub struct RebankManifest {
    pub name: String,
    pub version: u32,
    pub sample_rate: u32,
    pub rpm_min: f32,
    pub rpm_max: f32,
    pub parameters: Vec<String>,
    pub events: Vec<EventDesc>,
    pub master_volume: f32,
}

impl RebankManifest {
    pub fn new_engine_pack(name: &str, rpm_min: f32, rpm_max: f32) -> Self {
        Self {
            name: name.into(),
            version: 1,
            sample_rate: SAMPLE_RATE,
            rpm_min,
            rpm_max,
            parameters: vec![
                "rpms".into(),
                "throttle".into(),
                "state".into(),
                "boost".into(),
                "speed".into(),
            ],
            events: Vec::new(),
            master_volume: 1.0,
        }
    }

    pub fn to_json(&self) -> String {
        let mut events = String::new();
        for (ei, ev) in self.events.iter().enumerate() {
            if ei > 0 {
                events.push(',');
            }
            let mut layers = String::new();
            for (li, l) in ev.layers.iter().enumerate() {
                if li > 0 {
                    layers.push(',');
                }
                layers.push_str(&format!(
                    r#"
        {{"file":"{}","rpm_min":{},"rpm_max":{},"volume":{},"looped":{}}}"#,
                    l.file, l.rpm_min, l.rpm_max, l.volume, l.looped
                ));
            }
            let oneshot = ev
                .oneshot
                .as_ref()
                .map(|s| format!(r#","oneshot":"{s}""#))
                .unwrap_or_default();
            events.push_str(&format!(
                r#"
    {{
      "id":"{}",
      "category":"{}"{oneshot},
      "layers":[{layers}
      ]
    }}"#,
                ev.id, ev.category
            ));
        }
        let params: Vec<String> = self.parameters.iter().map(|p| format!("\"{p}\"")).collect();
        format!(
            r#"{{
  "name": "{}",
  "version": {},
  "format": "racingedge.rebank",
  "sample_rate": {},
  "rpm_min": {},
  "rpm_max": {},
  "master_volume": {},
  "parameters": [{}],
  "events": [{events}
  ]
}}
"#,
            self.name,
            self.version,
            self.sample_rate,
            self.rpm_min,
            self.rpm_max,
            self.master_volume,
            params.join(", "),
        )
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), String> {
        fs::write(path.as_ref(), self.to_json()).map_err(|e| e.to_string())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let text = fs::read_to_string(path.as_ref()).map_err(|e| e.to_string())?;
        Self::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Self, String> {
        let mut m = Self::new_engine_pack("unknown", 1000.0, 8000.0);
        if let Some(s) = json_str(text, "name") {
            m.name = s;
        }
        if let Some(v) = json_f32(text, "rpm_min") {
            m.rpm_min = v;
        }
        if let Some(v) = json_f32(text, "rpm_max") {
            m.rpm_max = v;
        }
        if let Some(v) = json_f32(text, "master_volume") {
            m.master_volume = v;
        }
        if let Some(v) = json_f32(text, "sample_rate") {
            m.sample_rate = v as u32;
        }
        // layer entries: rough scan for "file":"layers/...
        let mut layers_ext = Vec::new();
        let mut layers_int = Vec::new();
        let mut search = text;
        while let Some(idx) = search.find("\"file\"") {
            let after = &search[idx + 6..];
            let after = after.trim_start().trim_start_matches(':').trim_start();
            if !after.starts_with('"') {
                search = &search[idx + 6..];
                continue;
            }
            let end = after[1..].find('"').unwrap_or(0);
            let file = after[1..1 + end].to_string();
            let chunk = &search[idx..idx + 200.min(search.len() - idx)];
            let rmin = json_f32(chunk, "rpm_min").unwrap_or(0.0);
            let rmax = json_f32(chunk, "rpm_max").unwrap_or(9000.0);
            let vol = json_f32(chunk, "volume").unwrap_or(1.0);
            let desc = LayerDesc {
                file,
                rpm_min: rmin,
                rpm_max: rmax,
                volume: vol,
                looped: true,
            };
            if desc.file.contains("engine_int") {
                layers_int.push(desc);
            } else if desc.file.contains("engine_ext") {
                layers_ext.push(desc);
            }
            search = &search[idx + 6..];
        }
        if !layers_ext.is_empty() {
            m.events.push(EventDesc {
                id: "engine_ext".into(),
                category: "EngineExterior".into(),
                layers: layers_ext,
                oneshot: None,
            });
        }
        if !layers_int.is_empty() {
            m.events.push(EventDesc {
                id: "engine_int".into(),
                category: "EngineInterior".into(),
                layers: layers_int,
                oneshot: None,
            });
        }
        Ok(m)
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
    after
        .split([',', '}', '\n', ' '])
        .next()?
        .trim()
        .parse()
        .ok()
}

// =============================================================================
// WAV writer (PCM 16-bit mono) — zero dipendenze
// =============================================================================

pub fn write_wav_mono(
    path: impl AsRef<Path>,
    samples: &[f32],
    sample_rate: u32,
) -> Result<(), String> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut pcm = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        pcm.extend_from_slice(&v.to_le_bytes());
    }
    let data_len = pcm.len() as u32;
    let byte_rate = sample_rate * 2;
    let mut f = File::create(path).map_err(|e| e.to_string())?;
    f.write_all(b"RIFF").map_err(|e| e.to_string())?;
    f.write_all(&(36 + data_len).to_le_bytes())
        .map_err(|e| e.to_string())?;
    f.write_all(b"WAVE").map_err(|e| e.to_string())?;
    f.write_all(b"fmt ").map_err(|e| e.to_string())?;
    f.write_all(&16u32.to_le_bytes())
        .map_err(|e| e.to_string())?; // chunk size
    f.write_all(&1u16.to_le_bytes())
        .map_err(|e| e.to_string())?; // PCM
    f.write_all(&1u16.to_le_bytes())
        .map_err(|e| e.to_string())?; // mono
    f.write_all(&sample_rate.to_le_bytes())
        .map_err(|e| e.to_string())?;
    f.write_all(&byte_rate.to_le_bytes())
        .map_err(|e| e.to_string())?;
    f.write_all(&2u16.to_le_bytes())
        .map_err(|e| e.to_string())?; // block align
    f.write_all(&16u16.to_le_bytes())
        .map_err(|e| e.to_string())?; // bits
    f.write_all(b"data").map_err(|e| e.to_string())?;
    f.write_all(&data_len.to_le_bytes())
        .map_err(|e| e.to_string())?;
    f.write_all(&pcm).map_err(|e| e.to_string())?;
    Ok(())
}

pub fn read_wav_mono(path: impl AsRef<Path>) -> Result<(Vec<f32>, u32), String> {
    let data = fs::read(path.as_ref()).map_err(|e| e.to_string())?;
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err("not a WAV".into());
    }
    // find "data" chunk
    let mut i = 12usize;
    let mut sample_rate = SAMPLE_RATE;
    let mut bits = 16u16;
    let mut channels = 1u16;
    while i + 8 <= data.len() {
        let id = &data[i..i + 4];
        let size = u32::from_le_bytes(data[i + 4..i + 8].try_into().unwrap()) as usize;
        if id == b"fmt " && size >= 16 {
            channels = u16::from_le_bytes(data[i + 10..i + 12].try_into().unwrap());
            sample_rate = u32::from_le_bytes(data[i + 12..i + 16].try_into().unwrap());
            bits = u16::from_le_bytes(data[i + 22..i + 24].try_into().unwrap());
        } else if id == b"data" {
            let start = i + 8;
            let end = (start + size).min(data.len());
            let slice = &data[start..end];
            let mut samples = Vec::new();
            if bits == 16 {
                for c in slice.chunks_exact(2 * channels as usize) {
                    // mixdown to mono
                    let mut acc = 0.0f32;
                    for ch in 0..channels as usize {
                        let v = i16::from_le_bytes([c[ch * 2], c[ch * 2 + 1]]);
                        acc += v as f32 / 32768.0;
                    }
                    samples.push(acc / channels as f32);
                }
            }
            return Ok((samples, sample_rate));
        }
        i += 8 + size;
        if size % 2 == 1 {
            i += 1;
        }
    }
    Err("no data chunk".into())
}

// =============================================================================
// BUILDER — crea un .rebank da sintesi (o da layer esistenti)
// =============================================================================

pub struct RebankBuilder {
    pub name: String,
    pub rpm_min: f32,
    pub rpm_max: f32,
    pub out_dir: PathBuf,
}

impl RebankBuilder {
    pub fn new(name: impl Into<String>, out_parent: impl AsRef<Path>) -> Self {
        let name = name.into();
        let out_dir = out_parent.as_ref().join(format!("{name}.{REBANK_EXT}"));
        Self {
            name,
            rpm_min: 1200.0,
            rpm_max: 8000.0,
            out_dir,
        }
    }

    pub fn with_rpm_range(mut self, min: f32, max: f32) -> Self {
        self.rpm_min = min;
        self.rpm_max = max;
        self
    }

    /// Genera layer sintetici + oneshot SFX + manifest.
    pub fn build_from_synth(&self) -> Result<PathBuf, String> {
        fs::create_dir_all(self.out_dir.join("layers")).map_err(|e| e.to_string())?;
        fs::create_dir_all(self.out_dir.join("sfx")).map_err(|e| e.to_string())?;

        let lo = self.rpm_min;
        let hi = self.rpm_max.max(lo + 1000.0);
        let step = (hi - lo) / ENGINE_LAYER_COUNT as f32;

        let mut manifest = RebankManifest::new_engine_pack(&self.name, lo, hi);
        let mut ext_layers = Vec::new();
        let mut int_layers = Vec::new();

        for i in 0..ENGINE_LAYER_COUNT {
            let a = lo + i as f32 * step;
            let b = (a + step * 1.35).min(hi * 1.08);
            let layer = EngineLayer::synthesize(a, b, SAMPLE_RATE, 0.6);

            let ext_rel = format!("layers/engine_ext_{i:02}.wav");
            let int_rel = format!("layers/engine_int_{i:02}.wav");
            write_wav_mono(self.out_dir.join(&ext_rel), &layer.samples, SAMPLE_RATE)?;
            // interior: low-pass feel = softer harmonics (reuse with gain)
            let int_samples: Vec<f32> = layer.samples.iter().map(|s| s * 0.7).collect();
            write_wav_mono(self.out_dir.join(&int_rel), &int_samples, SAMPLE_RATE)?;

            ext_layers.push(LayerDesc {
                file: ext_rel,
                rpm_min: a,
                rpm_max: b,
                volume: 1.0,
                looped: true,
            });
            int_layers.push(LayerDesc {
                file: int_rel,
                rpm_min: a,
                rpm_max: b,
                volume: 0.85,
                looped: true,
            });
        }

        // oneshot sfx sintetici brevi
        self.write_oneshot_sfx("sfx/gear_shift.wav", 0.08, |t, _| {
            (t * 800.0 * std::f32::consts::TAU).sin() * (1.0 - t / 0.08).max(0.0)
        })?;
        self.write_oneshot_sfx("sfx/backfire.wav", 0.12, |t, i| {
            let n = ((i.wrapping_mul(1103515245) >> 16) as i16 as f32) / 32768.0;
            n * (1.0 - t / 0.12).max(0.0) * 2.0
        })?;
        self.write_oneshot_sfx("sfx/limiter.wav", 0.04, |t, _| {
            (t * 1200.0 * std::f32::consts::TAU).sin() * (1.0 - t / 0.04).max(0.0)
        })?;
        self.write_oneshot_sfx("sfx/wind.wav", 1.0, |t, i| {
            let n =
                ((i.wrapping_mul(1664525).wrapping_add(1013904223) >> 16) as i16 as f32) / 32768.0;
            n * 0.15 * (0.5 + 0.5 * (t * 0.5).sin())
        })?;

        manifest.events.push(EventDesc {
            id: "engine_ext".into(),
            category: format!("{:?}", SoundCategory::EngineExterior),
            layers: ext_layers,
            oneshot: None,
        });
        manifest.events.push(EventDesc {
            id: "engine_int".into(),
            category: format!("{:?}", SoundCategory::EngineInterior),
            layers: int_layers,
            oneshot: None,
        });
        for (id, file) in [
            ("gear_shift", "sfx/gear_shift.wav"),
            ("backfire", "sfx/backfire.wav"),
            ("limiter", "sfx/limiter.wav"),
            ("wind", "sfx/wind.wav"),
        ] {
            manifest.events.push(EventDesc {
                id: id.into(),
                category: id.into(),
                layers: vec![],
                oneshot: Some(file.into()),
            });
        }

        let man_path = self.out_dir.join("manifest.json");
        manifest.save(&man_path)?;
        eprintln!(
            "[rebank] built {} ({} layers × 2 + sfx)",
            self.out_dir.display(),
            ENGINE_LAYER_COUNT
        );
        Ok(self.out_dir.clone())
    }

    fn write_oneshot_sfx(
        &self,
        rel: &str,
        dur: f32,
        mut gen: impl FnMut(f32, usize) -> f32,
    ) -> Result<(), String> {
        let n = (SAMPLE_RATE as f32 * dur) as usize;
        let mut samples = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / SAMPLE_RATE as f32;
            samples.push(gen(t, i).clamp(-1.0, 1.0));
        }
        write_wav_mono(self.out_dir.join(rel), &samples, SAMPLE_RATE)
    }
}

/// Carica un `.rebank` in layer PCM pronti per `CarSound`.
pub fn load_rebank(
    path: impl AsRef<Path>,
) -> Result<(RebankManifest, Vec<EngineLayer>, Vec<EngineLayer>), String> {
    let root = path.as_ref();
    let man_path = if root.is_file() {
        root.to_path_buf()
    } else {
        root.join("manifest.json")
    };
    let root_dir = man_path
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let manifest = RebankManifest::load(&man_path)?;

    let mut ext = Vec::new();
    let mut int = Vec::new();
    for ev in &manifest.events {
        for l in &ev.layers {
            let wav = root_dir.join(&l.file);
            let (samples, sr) = read_wav_mono(&wav).unwrap_or_else(|_| {
                let syn = EngineLayer::synthesize(l.rpm_min, l.rpm_max, SAMPLE_RATE, 0.5);
                (syn.samples, SAMPLE_RATE)
            });
            let layer = EngineLayer {
                samples,
                sample_rate: sr,
                channels: 1,
                rpm_min: l.rpm_min,
                rpm_max: l.rpm_max,
                volume: l.volume,
                looped: l.looped,
            };
            if ev.id.contains("int") {
                int.push(layer);
            } else {
                ext.push(layer);
            }
        }
    }
    Ok((manifest, ext, int))
}

/// Path di default del rebank auto.
pub fn default_rebank_path() -> PathBuf {
    PathBuf::from("assets/sfx/vrc_pt_2024_ferrenzo_csp.rebank")
}

/// Assicura che esista un rebank (genera se manca).
pub fn ensure_default_rebank() -> PathBuf {
    let path = default_rebank_path();
    if path.join("manifest.json").exists() {
        return path;
    }
    // absolute fallback for sandbox
    let abs = PathBuf::from("/home/workdir/artifacts/racingedge").join(&path);
    if abs.join("manifest.json").exists() {
        return abs;
    }
    let parent = abs.parent().unwrap_or(Path::new("assets/sfx"));
    let name = "vrc_pt_2024_ferrenzo_csp";
    match RebankBuilder::new(name, parent)
        .with_rpm_range(1200.0, 8000.0)
        .build_from_synth()
    {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[rebank] generate failed: {e}");
            path
        }
    }
}
