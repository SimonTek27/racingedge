//! Bevy playback for the car audio mix models.
//!
//! [`CarSound`] and [`CommonSound`] compute the mix — engine layer crossfade,
//! cockpit/exterior split, tyre rolling/skid gain — but are pure models. This
//! plugin turns them into sound: every engine layer becomes a looping
//! [`AudioPlayer`], the tyre rolling and skid layers become two more, and a
//! system applies [`CarMix`] — the frame
//! [`build_mix_frame`](crate::simulator::audio_host::build_mix_frame) computes
//! once for every mixer — to each voice's [`AudioSink`] volume and playback
//! speed, so what you hear is the mix the port computes.
//!
//! A host with its own mixer reads the same frame instead of running this
//! plugin, which is why the feature is a swap rather than a rewrite.
//!
//! The engine set follows the camera: the `engine_int` layers are audible in
//! cockpit view and the `engine_ext` ones otherwise. Both sets are spawned and
//! the inactive one is held at zero gain, so switching camera never re-uploads
//! samples.

use bevy::audio::{AudioPlayer, AudioSink, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;

use crate::simulator::audio_host::{CarMix, TyreKind, VoiceId};
use crate::simulator::car_sound::CarSound;

/// Whether an engine voice is audible for the active camera.
///
/// Defined with the mix
/// ([`crate::simulator::audio_host::engine_voice_audible`]) so the virtual-voice
/// rule holds for every mixer; re-exported here where callers expect it.
pub use crate::simulator::audio_host::engine_voice_audible;

/// Sample rate used for the synthesised tyre loops.
pub const VOICE_SAMPLE_RATE: u32 = 44_100;
/// Length of the synthesised tyre loops, in seconds.
const TYRE_LOOP_SECONDS: f32 = 2.0;

/// One looping voice for one engine layer.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct EngineVoice {
    /// Layer index inside `CarSound::engine_int` / `CarSound::engine_ext`.
    pub layer: usize,
    /// The voice plays the interior (cockpit) layer set.
    pub interior: bool,
}

/// A looping tyre voice.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum TyreVoice {
    /// Rolling-contact rumble, louder and brighter with speed.
    Rolling,
    /// Sliding-skid hiss, driven by the surface slip.
    Skid,
}

/// Plugin that gives the car audio models audible voices.
pub struct CarAudioPlaybackPlugin;

impl Plugin for CarAudioPlaybackPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_voices).add_systems(
            Update,
            mix_voices.after(crate::simulator::audio_host::update_car_mix),
        );
    }
}

/// Spawn one looping player per engine layer plus the two tyre voices.
///
/// The engine layers come from [`CarSound`], so their content is whatever the
/// `.rebank` provided or the in-memory synthesis built. Without an audio plugin
/// (headless apps, tests) there is no asset store to load into and the system
/// does nothing.
fn spawn_voices(
    mut commands: Commands,
    sound: Res<CarSound>,
    sources: Option<ResMut<Assets<AudioSource>>>,
) {
    let Some(mut sources) = sources else {
        return;
    };

    for interior in [true, false] {
        let layers = if interior {
            &sound.engine_int
        } else {
            &sound.engine_ext
        };
        for (layer, data) in layers.iter().enumerate() {
            if data.samples.is_empty() || data.sample_rate == 0 {
                continue;
            }
            let samples = loopable(data.samples.clone(), data.sample_rate);
            let bytes = wav_bytes(&samples, data.sample_rate, data.channels.max(1));
            commands.spawn((
                AudioPlayer::new(sources.add(AudioSource {
                    bytes: bytes.into(),
                })),
                PlaybackSettings::LOOP,
                EngineVoice { layer, interior },
            ));
        }
    }

    for (voice, samples) in [
        (TyreVoice::Rolling, synth_tyre(false)),
        (TyreVoice::Skid, synth_tyre(true)),
    ] {
        let bytes = wav_bytes(&samples, VOICE_SAMPLE_RATE, 1);
        commands.spawn((
            AudioPlayer::new(sources.add(AudioSource {
                bytes: bytes.into(),
            })),
            PlaybackSettings::LOOP,
            voice,
        ));
    }
}

/// Applies the shared mix frame to every voice.
///
/// The frame is computed once in [`update_car_mix`] from the models; this system
/// owns only the `bevy_audio` side — volume, rate and the virtual-voice pause.
///
/// [`update_car_mix`]: crate::simulator::audio_host::update_car_mix
fn mix_voices(
    mix: Res<CarMix>,
    mut engine_voices: Query<(&EngineVoice, &mut AudioSink)>,
    mut tyre_voices: Query<(&TyreVoice, &mut AudioSink), Without<EngineVoice>>,
) {
    for (voice, mut sink) in &mut engine_voices {
        let id = VoiceId::Engine {
            layer: voice.layer,
            interior: voice.interior,
        };
        let Some(entry) = mix.0.voice(id) else {
            continue;
        };
        sink.set_volume(Volume::Linear(entry.gain));
        sink.set_speed(entry.rate);

        // Virtual voices: the set the camera does not select is paused rather
        // than mixed at zero gain, so a chase view never pays for eight silent
        // interior layers. This is the cheap half of what FMOD/Wwise call
        // virtual voices; priority-based stealing is the other half.
        if entry.audible {
            sink.play();
        } else {
            sink.pause();
        }
    }

    for (voice, mut sink) in &mut tyre_voices {
        let kind = match voice {
            TyreVoice::Rolling => TyreKind::Rolling,
            TyreVoice::Skid => TyreKind::Skid,
        };
        let Some(entry) = mix.0.voice(VoiceId::Tyre(kind)) else {
            continue;
        };
        sink.set_volume(Volume::Linear(entry.gain));
        sink.set_speed(entry.rate);
    }
}

/// Encode interleaved f32 PCM as a 16-bit RIFF/WAVE file Bevy can play.
///
/// # Example
///
/// ```
/// use racingedge::simulator::audio_playback::wav_bytes;
/// let bytes = wav_bytes(&[0.0, 0.5], 44_100, 1);
/// assert_eq!(&bytes[0..4], b"RIFF");
/// assert_eq!(&bytes[8..12], b"WAVE");
/// assert_eq!(bytes.len(), 44 + 4);
/// ```
pub fn wav_bytes(samples: &[f32], sample_rate: u32, channels: u16) -> Vec<u8> {
    let channels = channels.max(1);
    let bits_per_sample = 16u16;
    let block_align = channels * bits_per_sample / 8;
    let bytes_per_sample = u32::from(bits_per_sample / 8);
    let data_len = samples.len() as u32 * bytes_per_sample;
    let byte_rate = sample_rate * u32::from(block_align);

    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

/// Make a buffer loop without a seam: the first 50 ms are crossfaded with the
/// last 50 ms and the overlap is trimmed.
fn loopable(mut samples: Vec<f32>, sample_rate: u32) -> Vec<f32> {
    let fade = (sample_rate as usize / 20).max(1);
    let loop_len = samples.len().saturating_sub(fade);
    if loop_len == 0 {
        return samples;
    }
    for i in 0..fade {
        let t = i as f32 / fade as f32;
        samples[i] = samples[i] * t + samples[loop_len + i] * (1.0 - t);
    }
    samples.truncate(loop_len);
    samples
}

/// Deterministic noise in `-1..=1` (the port's LCG, as used by the engine synth).
fn noise(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1_103_515_245).wrapping_add(12_345);
    ((*state >> 16) as i16 as f32) / 32_768.0
}

/// Synthesise a seamless tyre loop: low rumble for rolling, hiss for skid.
fn synth_tyre(skid: bool) -> Vec<f32> {
    let sample_rate = VOICE_SAMPLE_RATE as usize;
    // Extra tail so `loopable` has an overlap to crossfade with the head.
    let count = (sample_rate as f32 * TYRE_LOOP_SECONDS) as usize + sample_rate / 12;
    let alpha = if skid { 0.55 } else { 0.06 };
    let gain = if skid { 0.55 } else { 0.35 };

    let mut state = if skid { 0x5EED_2A11 } else { 0x1BAD_5EED };
    let mut lowpass = 0.0f32;
    let mut samples = Vec::with_capacity(count);
    for _ in 0..count {
        let white = noise(&mut state);
        lowpass += (white - lowpass) * alpha;
        samples.push((lowpass * gain).clamp(-1.0, 1.0));
    }
    loopable(samples, sample_rate as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_voices_are_virtualised_by_camera() {
        // Cockpit: the interior set is audible, the exterior set is virtual.
        assert!(engine_voice_audible(true, true));
        assert!(!engine_voice_audible(false, true));
        // Chase: the mirror image, so exactly one set is ever mixed.
        assert!(!engine_voice_audible(true, false));
        assert!(engine_voice_audible(false, false));
    }

    #[test]
    fn wav_header_describes_pcm16() {
        let bytes = wav_bytes(&[0.0, 0.5, -0.5], 44_100, 1);
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(
            u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
            16,
            "fmt chunk size"
        );
        assert_eq!(
            u16::from_le_bytes([bytes[20], bytes[21]]),
            1,
            "PCM encoding"
        );
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 1, "mono");
        assert_eq!(
            u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
            44_100
        );
        assert_eq!(&bytes[36..40], b"data");
        assert_eq!(
            u32::from_le_bytes([bytes[40], bytes[41], bytes[42], bytes[43]]),
            6,
            "data chunk size"
        );
        assert_eq!(
            u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            36 + 6,
            "RIFF size"
        );
        assert_eq!(bytes.len(), 44 + 6);
    }

    #[test]
    fn wav_samples_clamp_and_stay_little_endian() {
        let bytes = wav_bytes(&[2.0, -2.0], 8_000, 1);
        assert_eq!(i16::from_le_bytes([bytes[44], bytes[45]]), i16::MAX);
        assert_eq!(i16::from_le_bytes([bytes[46], bytes[47]]), -i16::MAX);
    }

    #[test]
    fn stereo_wav_uses_two_channels() {
        let bytes = wav_bytes(&[0.0; 8], 22_050, 2);
        assert_eq!(u16::from_le_bytes([bytes[22], bytes[23]]), 2);
        assert_eq!(u16::from_le_bytes([bytes[32], bytes[33]]), 4, "block align");
        assert_eq!(
            u32::from_le_bytes([bytes[28], bytes[29], bytes[30], bytes[31]]),
            22_050 * 4,
            "byte rate"
        );
    }

    #[test]
    fn loopable_trims_the_overlap_and_matches_the_seam() {
        let ramp: Vec<f32> = (0..1_200).map(|i| i as f32).collect();
        // A 2 kHz "sample rate" gives a 100-sample crossfade.
        let looped = loopable(ramp, 2_000);
        assert_eq!(looped.len(), 1_100);
        // The head takes over from the tail, so the loop point is continuous.
        assert!((looped[0] - 1_100.0).abs() < 1e-3, "head = {}", looped[0]);
        assert!((looped[0] - looped[1_099]).abs() < 2.0);
    }

    #[test]
    fn loopable_handles_buffers_shorter_than_the_fade() {
        let samples = loopable(vec![0.5; 10], 44_100);
        assert_eq!(samples.len(), 10);
    }

    #[test]
    fn synth_tyre_is_deterministic_and_bounded() {
        let rolling = synth_tyre(false);
        assert_eq!(rolling, synth_tyre(false));
        assert!(!rolling.is_empty());
        assert!(rolling.iter().all(|s| s.is_finite() && s.abs() <= 1.0));
        let skid = synth_tyre(true);
        assert_ne!(rolling, skid);
        let peak = |v: &[f32]| v.iter().fold(0.0f32, |m, s| m.max(s.abs()));
        assert!(peak(&skid) > peak(&rolling));
    }

    #[test]
    fn synth_tyre_loops_without_a_jump() {
        let samples = synth_tyre(false);
        let first = samples[0];
        let last = samples[samples.len() - 1];
        assert!((first - last).abs() < 0.05, "seam {first} -> {last}");
    }
}
