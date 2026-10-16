//! Audio bank integration for racingedge.
//!
//! This module bridges the FSB5 modding tools ([`crate::editor::audio::fsb`] and
//! [`crate::editor::audio::vorbis`]) into Bevy: FSB banks can be loaded at runtime and
//! their samples expanded into playable [`bevy::audio::AudioSource`] assets —
//! RIFF/WAVE for uncompressed PCM banks, Ogg Vorbis for Vorbis banks.
//!
//! The FSB parsing itself lives in the library crate so the game and the
//! `rea_bridge` CLI share exactly one implementation.

use std::sync::Arc;

use bevy::audio::{AudioBundle, AudioSource as BevyAudioSource, PlaybackSettings};
use bevy::prelude::*;

use crate::editor::audio::fsb::{Fsb5, SoundFormat};
use crate::editor::audio::vorbis;

/// A sample converted into a self-contained, playable audio file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayableSample {
    /// Container extension: `wav` or `ogg`.
    pub extension: &'static str,
    /// Complete file bytes.
    pub bytes: Vec<u8>,
}

/// Convert one FSB sample into a playable file.
///
/// PCM banks become RIFF/WAVE and Vorbis banks become Ogg Vorbis. Codecs that
/// require an FMOD-specific decoder (MPEG, XMA, GCADPCM, …) return an error.
pub fn sample_to_playable(bank: &Fsb5, index: usize) -> Result<PlayableSample, String> {
    if let Some(bytes) = bank.sample_to_wav(index) {
        return Ok(PlayableSample {
            extension: "wav",
            bytes,
        });
    }
    if bank.format == SoundFormat::Vorbis {
        let bytes = vorbis::build_ogg(bank, index).map_err(|error| error.to_string())?;
        return Ok(PlayableSample {
            extension: "ogg",
            bytes,
        });
    }
    Err(format!(
        "bank format {} cannot be converted to a playable file",
        bank.format
    ))
}

/// A lightweight, UI-friendly description of one bank sample.
#[derive(Debug, Clone, PartialEq)]
pub struct BankSampleSummary {
    /// Zero-based sample index.
    pub index: usize,
    /// Sample name.
    pub name: String,
    /// Sample rate in hertz.
    pub frequency: u32,
    /// Channel count.
    pub channels: u32,
    /// Frame count.
    pub num_frames: u32,
    /// Duration in seconds.
    pub duration_seconds: f32,
    /// Whether the sample loops.
    pub looped: bool,
    /// Payload size in bytes.
    pub bytes: usize,
    /// Container it can be played as, when the codec is supported.
    pub playable_as: Option<&'static str>,
}

/// Describe every sample in a bank.
pub fn bank_summaries(bank: &Fsb5) -> Vec<BankSampleSummary> {
    bank.samples
        .iter()
        .map(|sample| BankSampleSummary {
            index: sample.index,
            name: sample.name.clone(),
            frequency: sample.frequency,
            channels: sample.channels,
            num_frames: sample.num_frames,
            duration_seconds: sample.duration_seconds(),
            looped: sample.is_looped(),
            bytes: bank.sample_data(sample.index).map(|d| d.len()).unwrap_or(0),
            playable_as: playable_extension(bank, sample.index),
        })
        .collect()
}

/// The container extension a sample can be rebuilt as, if any.
fn playable_extension(bank: &Fsb5, index: usize) -> Option<&'static str> {
    if bank.sample_to_wav(index).is_some() {
        return Some("wav");
    }
    if bank.format == SoundFormat::Vorbis && vorbis_is_exportable(bank, index) {
        return Some("ogg");
    }
    None
}

/// Whether a Vorbis sample has both packets and a known setup header, i.e.
/// whether [`sample_to_playable`] can actually rebuild it as Ogg.
fn vorbis_is_exportable(bank: &Fsb5, index: usize) -> bool {
    let Some(crc32) = bank
        .samples
        .get(index)
        .and_then(|sample| sample.vorbis_data())
        .map(|data| data.setup_crc32)
    else {
        return false;
    };
    vorbis::setup_headers().get(crc32).is_some() && bank.vorbis_packets(index).is_some()
}

/// An FSB bank loaded into the game, with a playable handle per sample.
#[derive(Debug, Clone)]
pub struct LoadedBank {
    /// Path the bank was loaded from.
    pub path: String,
    /// Bank audio format.
    pub format: SoundFormat,
    /// Per-sample descriptions.
    pub summaries: Vec<BankSampleSummary>,
    /// Playable asset handle per sample (`None` for unsupported codecs).
    pub handles: Vec<Option<Handle<BevyAudioSource>>>,
}

impl LoadedBank {
    /// Playable sample count.
    pub fn playable_count(&self) -> usize {
        self.handles.iter().filter(|handle| handle.is_some()).count()
    }

    /// Find a sample index by name.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.summaries
            .iter()
            .position(|summary| summary.name == name)
    }
}

/// Resource holding every FSB bank loaded into the running game.
#[derive(Resource, Default)]
pub struct BankLibrary {
    /// Loaded banks, in load order.
    pub banks: Vec<LoadedBank>,
}

impl BankLibrary {
    /// Total number of loaded banks.
    pub fn len(&self) -> usize {
        self.banks.len()
    }

    /// Whether no banks are loaded.
    pub fn is_empty(&self) -> bool {
        self.banks.is_empty()
    }
}

/// Load an FSB bank from disk, adding a playable asset for every supported
/// sample and recording its summaries. Returns the number of playable samples.
pub fn load_bank(
    path: &str,
    audio_assets: &mut Assets<BevyAudioSource>,
    library: &mut BankLibrary,
) -> Result<usize, String> {
    let bank = Fsb5::from_file(path).map_err(|error| error.to_string())?;
    let summaries = bank_summaries(&bank);

    let mut handles = Vec::with_capacity(bank.len());
    for index in 0..bank.len() {
        match sample_to_playable(&bank, index) {
            Ok(playable) => {
                let handle = audio_assets.add(BevyAudioSource {
                    bytes: Arc::from(playable.bytes.into_boxed_slice()),
                });
                handles.push(Some(handle));
            }
            Err(_) => handles.push(None),
        }
    }

    let playable = handles.iter().filter(|handle| handle.is_some()).count();
    library.banks.push(LoadedBank {
        path: path.to_string(),
        format: bank.format,
        summaries,
        handles,
    });
    Ok(playable)
}

/// Spawn an audio player for a specific sample, if it has a playable handle.
pub fn play_sample(
    commands: &mut Commands,
    library: &BankLibrary,
    bank_index: usize,
    sample_index: usize,
    looped: bool,
) -> bool {
    let Some(bank) = library.banks.get(bank_index) else {
        return false;
    };
    let Some(Some(handle)) = bank.handles.get(sample_index) else {
        return false;
    };
    let settings = if looped {
        PlaybackSettings::LOOP
    } else {
        PlaybackSettings::ONCE
    };
    commands.spawn(AudioBundle {
        source: handle.clone(),
        settings,
    });
    true
}

/// Plugin that loads configured banks and exposes preview controls.
pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<BankLibrary>()
            .register_type::<AudioSource>()
            .register_type::<FmodSoundBank>()
            .register_type::<FmodSampleInfo>()
            .add_systems(Startup, (spawn_audio, load_configured_banks))
            .add_systems(Update, (update_audio, preview_banks))
            .add_plugins(crate::editor::audio::browser::BankBrowserPlugin);
    }
}

/// Spawn a default audio source component.
fn spawn_audio(mut commands: Commands) {
    commands.spawn(AudioSource {
        name: "default".to_string(),
        ..default()
    });
}

fn update_audio(_time: Res<Time>, mut query: Query<(&mut AudioSource,)>) {
    for _audio in &mut query {
        // Placeholder: parameter automation would be applied here.
    }
}

/// Bank paths configured via `RACINGEDGE_BANKS` (semicolon-separated), or every
/// `.fsb` found under `assets/audio`.
fn configured_bank_paths() -> Vec<String> {
    let mut paths = Vec::new();
    if let Ok(value) = std::env::var("RACINGEDGE_BANKS") {
        for part in value.split(';') {
            let part = part.trim();
            if !part.is_empty() {
                paths.push(part.to_string());
            }
        }
    }
    if paths.is_empty() {
        if let Ok(entries) = std::fs::read_dir("assets/audio") {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_fsb = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension.eq_ignore_ascii_case("fsb"))
                    .unwrap_or(false);
                if is_fsb {
                    paths.push(path.to_string_lossy().to_string());
                }
            }
        }
    }
    paths
}

fn load_configured_banks(
    mut audio_assets: ResMut<Assets<BevyAudioSource>>,
    mut library: ResMut<BankLibrary>,
) {
    for path in configured_bank_paths() {
        match load_bank(&path, &mut audio_assets, &mut library) {
            Ok(count) => info!("racingedge: loaded bank {path} ({count} playable sample(s))"),
            Err(error) => warn!("racingedge: could not load bank {path}: {error}"),
        }
    }
}

/// Press `P` to preview the first playable sample of the first loaded bank.
fn preview_banks(keys: Res<Input<KeyCode>>, library: Res<BankLibrary>, mut commands: Commands) {
    if !keys.just_pressed(KeyCode::P) {
        return;
    }
    let Some(bank) = library.banks.first() else {
        return;
    };
    for (index, handle) in bank.handles.iter().enumerate() {
        if handle.is_some() {
            play_sample(&mut commands, &library, 0, index, false);
            break;
        }
    }
}

// --- Modding data model -------------------------------------------------
//
// These components describe FMOD content discovered via REA re-engineering and
// are attached to game entities; the FSB5 parser above is what actually reads
// real bank files.

/// Component representing an audio source that can be played or modified.
#[derive(Component, Default, Reflect)]
pub struct AudioSource {
    /// Name of the audio cue (e.g. "engine roar", "tire squeal").
    pub name: String,
    /// Whether the audio is looped.
    pub looped: bool,
    /// Relative volume (0.0 to 1.0).
    pub volume: f32,
    /// Pitch shift in semitones.
    pub pitch: f32,
    /// Sample index/offset within the source bank.
    pub fmod_sample_id: u32,
    /// Whether this audio should override the original FMOD sample.
    pub override_original: bool,
}

/// FMOD sound bank metadata.
#[derive(Component, Default, Reflect)]
pub struct FmodSoundBank {
    /// Path to the `.fsb` sound bank file.
    pub fsb_path: String,
    /// Number of samples in the bank.
    pub sample_count: u32,
    /// Base path where `.fxb` effect files are located.
    pub effects_path: String,
    /// Sample structures discovered by analysis.
    pub sample_structures: Vec<FmodSampleInfo>,
    /// Whether the bank uses compressed audio.
    pub uses_compression: bool,
    /// Compression format if applicable (vorbis, mp3, …).
    pub compression_format: String,
}

/// Information about a single sample.
#[derive(Component, Default, Reflect, Clone)]
pub struct FmodSampleInfo {
    /// Sample name.
    pub name: String,
    /// Offset in the bank (bytes).
    pub file_offset: u64,
    /// Size in bytes.
    pub file_size: u64,
    /// Loop start frame.
    pub loop_start: u64,
    /// Loop end frame.
    pub loop_end: u64,
    /// Sample type/format.
    pub sample_type: String,
    /// Bit rate if applicable.
    pub bit_rate: Option<u32>,
    /// Whether the sample loops.
    pub loop_type: String,
}

/// Channel/parameter information for a sample.
#[derive(Component, Default, Reflect)]
pub struct FmodChannelParams {
    /// Volume modulation curve.
    pub volume_curve: Vec<f32>,
    /// Pitch modulation curve.
    pub pitch_curve: Vec<f32>,
    /// DSP effect parameters.
    pub dsp_params: Vec<f32>,
    /// Spatialization settings.
    pub spatialization: Vec3,
    /// Whether the channel uses automation.
    pub uses_automation: bool,
    /// Automation curve points as `(time, value)`.
    pub automation_points: Vec<(f32, f32)>,
}

/// An FMOD `.fxb` effect bank.
#[derive(Component, Default, Reflect)]
pub struct FmodEffectBank {
    /// Path to the `.fxb` file.
    pub fxb_path: String,
    /// Number of effects defined in the bank.
    pub effect_count: u32,
    /// Effect types (distortion, reverb, …).
    pub effect_types: Vec<String>,
    /// Whether effects are enabled by default.
    pub enabled: bool,
}

/// Build a [`FmodSampleInfo`] from a parsed sample.
pub fn sample_info_from_bank(bank: &Fsb5, index: usize) -> Option<FmodSampleInfo> {
    let sample = bank.samples.get(index)?;
    Some(FmodSampleInfo {
        name: sample.name.clone(),
        file_offset: sample.data_offset as u64,
        file_size: bank.sample_data(index).map(|d| d.len() as u64).unwrap_or(0),
        loop_start: sample.loop_start.unwrap_or(0) as u64,
        loop_end: sample.loop_end.unwrap_or(0) as u64,
        sample_type: bank.format.to_string(),
        bit_rate: None,
        loop_type: if sample.is_looped() {
            "loop".to_string()
        } else {
            "one_shot".to_string()
        },
    })
}

/// Apply discovered parameters to an audio source component.
pub fn play_sample_with_params(
    commands: &mut Commands,
    audio_source: &mut AudioSource,
    sample_info: &FmodSampleInfo,
    channel_params: &FmodChannelParams,
) {
    audio_source.name = sample_info.name.clone();
    audio_source.looped = sample_info.loop_type != "one_shot";
    audio_source.volume = channel_params.volume_curve.first().copied().unwrap_or(1.0);
    audio_source.pitch = channel_params.pitch_curve.first().copied().unwrap_or(1.0);
    audio_source.fmod_sample_id = sample_info.file_offset as u32;
    audio_source.override_original = true;
    commands.spawn(AudioSource::default());
}

/// Convert REA-discovered pseudocode into a Rust audio-processing skeleton.
pub fn rea_to_rust_audio_code(function_desc: &str) -> String {
    let mut result = String::new();
    result.push_str("// Generated from REA pseudocode analysis\n");
    result.push_str("// Source: REA native binary analysis\n\n");

    let lowered = function_desc.to_lowercase();
    if lowered.contains("play") || lowered.contains("sample") {
        result.push_str("// FMOD sample playback -> Bevy audio equivalent\n");
        result.push_str("// BEFORE (pseudocode from REA):\n");
        result.push_str("//   void PlaySample(FmodSample* sample, FmodChannel* channel) {\n");
        result.push_str("//       sample->Play(channel->volume, channel->pitch);\n");
        result.push_str("//       if (channel->dsp) { ApplyDSP(channel->dsp); }\n");
        result.push_str("//   }\n\n");
        result.push_str("// AFTER (Rust/Bevy):\n");
        result.push_str("// audio_source.volume = channel_params.volume_curve.first().copied().unwrap_or(1.0);\n");
        result.push_str("// audio_source.pitch  = channel_params.pitch_curve.first().copied().unwrap_or(1.0);\n");
        result.push_str("// audio_source.looped = sample_info.loop_type != \"one_shot\";\n");
    } else {
        result.push_str("// No automatic translation matched; review the pseudocode manually.\n");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::audio::fsb_write::{Fsb5Builder, PcmSample};

    fn pcm_bank() -> Fsb5 {
        let bytes = Fsb5Builder::new(SoundFormat::Pcm16)
            .with_sample(PcmSample::new("engine", 44_100, 2, vec![0u8; 128]))
            .with_sample(PcmSample::new("horn", 22_050, 1, vec![0u8; 32]).with_loop(0, 16))
            .build()
            .expect("build");
        Fsb5::parse(&bytes).expect("parse")
    }

    #[test]
    fn converts_pcm_sample_to_wav() {
        let bank = pcm_bank();
        let playable = sample_to_playable(&bank, 0).expect("playable");
        assert_eq!(playable.extension, "wav");
        assert_eq!(&playable.bytes[0..4], b"RIFF");
        assert_eq!(&playable.bytes[8..12], b"WAVE");
    }

    #[test]
    fn summaries_describe_every_sample() {
        let bank = pcm_bank();
        let summaries = bank_summaries(&bank);
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].name, "engine");
        assert_eq!(summaries[0].channels, 2);
        assert_eq!(summaries[0].playable_as, Some("wav"));
        assert!(summaries[1].looped);
        assert!(summaries[1].duration_seconds > 0.0);
    }

    /// A minimal single-sample VORBIS bank with one audio packet and the
    /// bundled setup header (id 118203318).
    fn vorbis_bank() -> Fsb5 {
        let mut file = Vec::new();
        file.extend_from_slice(b"FSB5");
        file.extend_from_slice(&1u32.to_le_bytes()); // version
        file.extend_from_slice(&1u32.to_le_bytes()); // numSamples
        file.extend_from_slice(&24u32.to_le_bytes()); // sampleHeaderSize
        file.extend_from_slice(&0u32.to_le_bytes()); // nameTableSize
        file.extend_from_slice(&6u32.to_le_bytes()); // dataSize
        file.extend_from_slice(&15u32.to_le_bytes()); // VORBIS
        file.extend_from_slice(&[0u8; 8]);
        file.extend_from_slice(&[0u8; 16]);
        file.extend_from_slice(&[0u8; 8]);
        let raw: u64 = 1 | (8u64 << 1); // extraParams, 44100, mono
        file.extend_from_slice(&raw.to_le_bytes());
        let chunk: u32 = (12u32 << 1) | (11u32 << 25); // VORBISDATA
        file.extend_from_slice(&chunk.to_le_bytes());
        file.extend_from_slice(&118_203_318u32.to_le_bytes()); // setup id
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&[0x02, 0x00, 0xAA, 0xBB, 0x00, 0x00]);
        Fsb5::parse(&file).expect("parse")
    }

    #[test]
    fn converts_vorbis_sample_to_ogg() {
        let bank = vorbis_bank();
        assert_eq!(bank_summaries(&bank)[0].playable_as, Some("ogg"));
        let playable = sample_to_playable(&bank, 0).expect("playable");
        assert_eq!(playable.extension, "ogg");
        assert_eq!(&playable.bytes[0..4], b"OggS");
    }

    /// A hand-built bank whose codec has no built-in converter.
    fn mpeg_bank() -> Fsb5 {
        let mut file = Vec::new();
        file.extend_from_slice(b"FSB5");
        file.extend_from_slice(&1u32.to_le_bytes()); // version
        file.extend_from_slice(&1u32.to_le_bytes()); // numSamples
        file.extend_from_slice(&8u32.to_le_bytes()); // sampleHeaderSize
        file.extend_from_slice(&0u32.to_le_bytes()); // nameTableSize
        file.extend_from_slice(&4u32.to_le_bytes()); // dataSize
        file.extend_from_slice(&11u32.to_le_bytes()); // MPEG
        file.extend_from_slice(&[0u8; 8]);
        file.extend_from_slice(&[0u8; 16]);
        file.extend_from_slice(&[0u8; 8]);
        let raw: u64 = 8u64 << 1; // 44100, mono
        file.extend_from_slice(&raw.to_le_bytes());
        file.extend_from_slice(&[0u8; 4]);
        Fsb5::parse(&file).expect("parse")
    }

    #[test]
    fn unsupported_codec_reports_error() {
        let bank = mpeg_bank();
        assert!(sample_to_playable(&bank, 0).is_err());
        assert_eq!(bank_summaries(&bank)[0].playable_as, None);
    }

    #[test]
    fn sample_info_reflects_loop_metadata() {
        let bank = pcm_bank();
        let info = sample_info_from_bank(&bank, 1).expect("info");
        assert_eq!(info.name, "horn");
        assert_eq!(info.loop_type, "loop");
        assert_eq!(info.loop_start, 0);
        assert_eq!(info.loop_end, 16);
        assert_eq!(info.sample_type, "pcm16");
    }
}
