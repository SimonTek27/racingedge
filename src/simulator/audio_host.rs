//! The car audio mix as plain data, for any mixer.
//!
//! Port of the mix in ksengine `src/simulator/SimulatorAudio.*` /
//! `VehicleAudioHook.h` (GPL-3.0). Original:
//! <https://github.com/SimonTek27/ksengine>
//!
//! [`CarSound`] and [`CommonSound`] decide what the car should sound like: the
//! engine layer crossfade, the cockpit/exterior split, the tyre rolling and skid
//! levels. Turning that into audible voices used to happen only inside
//! `bevy_audio` ([`crate::simulator::audio_playback`]); this module computes the
//! same decision once, as a [`CarMixFrame`] of per-voice gain, playback rate and
//! audibility, with no audio API in sight.
//!
//! * the Bevy path applies the frame to `AudioSink`s, and
//! * a host with its own mixer — Renzora's `renzora_audio` — reads the very same
//!   frame from the [`CarMix`] resource and drives its own voices.
//!
//! So both sides hear identical numbers, and a host does not have to re-derive
//! the mix (or link `bevy_audio`) to do it. See `docs/RENZORA_MIGRATION.md`.
//!
//! # Example
//!
//! ```
//! use racingedge::simulator::audio_host::{build_mix_frame, TyreKind, VoiceId};
//! use racingedge::simulator::car_sound::{CameraMode, CarSound, CommonSound, TrackSurface};
//!
//! let mut sound = CarSound::default();
//! sound.camera = CameraMode::Chase;
//! sound.state.rpm = 4000.0;
//! sound.state.speed = 30.0;
//! let mut common = CommonSound::default();
//! common.set_surface(TrackSurface::Asphalt, 30.0, 1.0);
//!
//! let frame = build_mix_frame(&sound, &common);
//!
//! // Chase view plays the exterior engine set and pauses the interior one.
//! assert!(frame
//!     .voices()
//!     .iter()
//!     .filter(|v| matches!(v.id, VoiceId::Engine { interior: true, .. }))
//!     .all(|v| !v.audible));
//! // A locked tyre is full skid.
//! assert_eq!(
//!     frame.voice(VoiceId::Tyre(TyreKind::Skid)).map(|v| v.gain),
//!     Some(0.8)
//! );
//! ```

use bevy::prelude::*;

use super::car_sound::{CameraMode, CarSound, CommonSound, TrackSurface};

/// One voice of the car mix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VoiceId {
    /// An engine layer: `layer` indexes `CarSound::engine_int` / `engine_ext`,
    /// and `interior` selects which of the two sets it belongs to.
    Engine {
        /// Layer index inside the engine set.
        layer: usize,
        /// The voice plays the interior (cockpit) set.
        interior: bool,
    },
    /// One of the two tyre loops.
    Tyre(TyreKind),
}

/// The tyre loops, as they are spawned and mixed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TyreKind {
    /// Rolling-contact rumble.
    Rolling,
    /// Sliding-skid hiss.
    Skid,
}

impl TyreKind {
    /// Every tyre voice, in mix order.
    pub const ALL: [TyreKind; 2] = [Self::Rolling, Self::Skid];

    /// The [`CommonSound`] category this loop's level comes from.
    pub fn category(self, surface: TrackSurface) -> usize {
        match self {
            Self::Rolling => surface.rolling() as usize,
            Self::Skid => surface.skid() as usize,
        }
    }

    /// Playback rate for a ground speed in m/s.
    ///
    /// Rolling rises with speed (a faster tyre hums higher); skid starts near
    /// unity so a lock-up is audible even when the car is slow.
    pub fn rate(self, speed_ms: f32) -> f32 {
        match self {
            Self::Rolling => (0.5 + speed_ms / 80.0).clamp(0.5, 1.5),
            Self::Skid => (0.9 + speed_ms / 120.0).clamp(0.9, 1.6),
        }
    }
}

/// Whether an engine voice is audible for the active camera: the interior set
/// in cockpit view, the exterior set otherwise.
///
/// Kept as a free function so the virtual-voice decision is testable without an
/// audio device or a mix frame.
pub fn engine_voice_audible(interior: bool, cockpit: bool) -> bool {
    interior == cockpit
}

/// One computed voice: what to play, how loud, how fast, and whether it should
/// be running at all.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VoiceMix {
    /// Which voice this is.
    pub id: VoiceId,
    /// Linear gain, `0..=1`.
    pub gain: f32,
    /// Playback rate (`1.0` = as recorded).
    pub rate: f32,
    /// `false` for a voice the current camera/mode does not select: a mixer
    /// should pause it rather than run it at zero gain (a virtual voice).
    pub audible: bool,
}

/// The whole car mix for one frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CarMixFrame {
    voices: Vec<VoiceMix>,
}

impl CarMixFrame {
    /// Every voice in the frame.
    pub fn voices(&self) -> &[VoiceMix] {
        &self.voices
    }

    /// The mix for one voice, when that voice exists in this car.
    pub fn voice(&self, id: VoiceId) -> Option<&VoiceMix> {
        self.voices.iter().find(|voice| voice.id == id)
    }

    /// The gain for one voice (`0.0` when it does not exist).
    pub fn gain(&self, id: VoiceId) -> f32 {
        self.voice(id).map(|voice| voice.gain).unwrap_or(0.0)
    }

    /// How many voices are audible this frame.
    pub fn audible_count(&self) -> usize {
        self.voices.iter().filter(|voice| voice.audible).count()
    }
}

/// Computes the mix for the current car and surface.
///
/// The numbers are the ones the `bevy_audio` path used to compute inline, so
/// the frame is the single definition of the mix. Engine layers with no audio
/// (empty sample buffer) are left out, exactly as the voices are spawned.
pub fn build_mix_frame(sound: &CarSound, common: &CommonSound) -> CarMixFrame {
    let rpm = sound.state.rpm;
    let engine_gain = sound.engine_gain();
    let cockpit = sound.camera == CameraMode::Cockpit;

    let mut voices = Vec::with_capacity(2 * sound.engine_int.len() + TyreKind::ALL.len());

    for (interior, layers) in [(true, &sound.engine_int), (false, &sound.engine_ext)] {
        for (layer, data) in layers.iter().enumerate() {
            if data.samples.is_empty() || data.sample_rate == 0 {
                continue;
            }
            let audible = engine_voice_audible(interior, cockpit) && sound.enabled;
            let gain = if audible {
                (engine_gain * data.weight_at_rpm(rpm) * data.volume).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mid_rpm = ((data.rpm_min + data.rpm_max) * 0.5).max(1.0);
            voices.push(VoiceMix {
                id: VoiceId::Engine { layer, interior },
                gain,
                rate: (rpm / mid_rpm).clamp(0.5, 2.0),
                audible,
            });
        }
    }

    for kind in TyreKind::ALL {
        let category = kind.category(common.active_surface);
        let gain = if common.enabled && sound.enabled {
            common
                .volumes
                .get(category)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, 1.0)
        } else {
            0.0
        };
        voices.push(VoiceMix {
            id: VoiceId::Tyre(kind),
            gain,
            rate: kind.rate(sound.state.speed),
            // Tyre loops always run; a still car simply has no level. A mixer
            // that prefers pausing silent loops can still do so from `gain`.
            audible: gain > 0.0,
        });
    }

    CarMixFrame { voices }
}

/// The current [`CarMixFrame`], as a resource a host mixer can read.
///
/// Refreshed by [`update_car_mix`]; a host that replaces `bevy_audio` (Renzora)
/// reads it instead of computing the mix again.
#[derive(Resource, Default, Clone, Debug)]
pub struct CarMix(pub CarMixFrame);

/// Recomputes [`CarMix`] whenever the models change.
///
/// Cheap: it is a handful of floats per voice, and the frame is only replaced
/// when it actually differs, so a host that compares frames sees no churn.
pub fn update_car_mix(sound: Res<CarSound>, common: Res<CommonSound>, mut mix: ResMut<CarMix>) {
    let frame = build_mix_frame(&sound, &common);
    if mix.0 != frame {
        mix.0 = frame;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulator::car_sound::{AudioPhysicsState, CommonSoundCategory};

    fn cockpit() -> CarSound {
        CarSound {
            camera: CameraMode::Cockpit,
            state: AudioPhysicsState {
                rpm: 4000.0,
                speed: 30.0,
                throttle: 1.0,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn chase() -> CarSound {
        CarSound {
            camera: CameraMode::Chase,
            ..cockpit()
        }
    }

    #[test]
    fn the_camera_selects_one_engine_set() {
        let mut common = CommonSound::default();
        common.set_surface(TrackSurface::Asphalt, 30.0, 0.0);

        let interior = build_mix_frame(&cockpit(), &common);
        assert!(interior
            .voices()
            .iter()
            .filter(|v| matches!(v.id, VoiceId::Engine { interior: true, .. }))
            .all(|v| v.audible));
        assert!(interior
            .voices()
            .iter()
            .filter(|v| matches!(
                v.id,
                VoiceId::Engine {
                    interior: false,
                    ..
                }
            ))
            .all(|v| !v.audible && v.gain == 0.0));

        let exterior = build_mix_frame(&chase(), &common);
        assert!(exterior
            .voices()
            .iter()
            .filter(|v| matches!(
                v.id,
                VoiceId::Engine {
                    interior: false,
                    ..
                }
            ))
            .all(|v| v.audible));
        assert!(exterior
            .voices()
            .iter()
            .filter(|v| matches!(v.id, VoiceId::Engine { interior: true, .. }))
            .all(|v| !v.audible && v.gain == 0.0));
    }

    #[test]
    fn every_voice_is_present_exactly_once() {
        let frame = build_mix_frame(&chase(), &CommonSound::default());
        let mut ids: Vec<VoiceId> = frame.voices().iter().map(|v| v.id).collect();
        let total = ids.len();
        ids.sort_by_key(|id| format!("{id:?}"));
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate voice id");

        // Two engine sets, two tyre loops.
        assert_eq!(total, 2 * CarSound::default().engine_int.len() + 2);
        assert!(frame.voice(VoiceId::Tyre(TyreKind::Rolling)).is_some());
        assert!(frame.voice(VoiceId::Tyre(TyreKind::Skid)).is_some());
        assert!(frame
            .voice(VoiceId::Engine {
                layer: 0,
                interior: true
            })
            .is_some());
    }

    #[test]
    fn unknown_voices_have_no_gain() {
        let frame = build_mix_frame(&chase(), &CommonSound::default());
        assert_eq!(
            frame.gain(VoiceId::Engine {
                layer: 99,
                interior: false
            }),
            0.0
        );
        assert_eq!(frame.gain(VoiceId::Tyre(TyreKind::Skid)), 0.0);
        assert!(frame
            .voice(VoiceId::Engine {
                layer: 99,
                interior: true
            })
            .is_none());
    }

    #[test]
    fn disabling_the_car_silences_every_voice() {
        let mut sound = chase();
        sound.enabled = false;
        let mut common = CommonSound::default();
        common.set_surface(TrackSurface::Asphalt, 40.0, 1.0);

        let frame = build_mix_frame(&sound, &common);
        assert_eq!(frame.audible_count(), 0);
        assert!(frame.voices().iter().all(|v| v.gain == 0.0));
    }

    #[test]
    fn disabling_the_surface_layers_silences_the_tyres_only() {
        let mut common = CommonSound::default();
        common.set_surface(TrackSurface::Asphalt, 40.0, 1.0);
        common.enabled = false;

        let frame = build_mix_frame(&chase(), &common);
        assert_eq!(frame.gain(VoiceId::Tyre(TyreKind::Skid)), 0.0);
        assert_eq!(frame.gain(VoiceId::Tyre(TyreKind::Rolling)), 0.0);
        assert!(
            frame
                .voices()
                .iter()
                .filter(|v| matches!(v.id, VoiceId::Engine { .. }))
                .any(|v| v.gain > 0.0),
            "the engine is unaffected by the surface layers"
        );
    }

    #[test]
    fn tyre_levels_come_from_the_surface_volumes() {
        let mut common = CommonSound::default();
        common.set_surface(TrackSurface::Asphalt, 30.0, 1.0);
        let frame = build_mix_frame(&chase(), &common);

        let skid = frame.voice(VoiceId::Tyre(TyreKind::Skid)).expect("skid");
        assert_eq!(skid.gain, 0.8);
        assert!(skid.audible);

        let rolling = frame
            .voice(VoiceId::Tyre(TyreKind::Rolling))
            .expect("rolling");
        assert!(rolling.gain > 0.0 && rolling.gain < skid.gain);

        // The category is resolved through the active surface, not the name.
        assert_eq!(
            TrackSurface::Grass.rolling() as usize,
            CommonSoundCategory::SurfaceGrass as usize
        );
        assert_eq!(
            TrackSurface::Grass.skid() as usize,
            CommonSoundCategory::SkidGrass as usize
        );
    }

    #[test]
    fn engine_rate_follows_rpm_within_bounds() {
        let mut sound = chase();
        let common = CommonSound::default();

        sound.state.rpm = 1200.0;
        let low = build_mix_frame(&sound, &common)
            .voice(VoiceId::Engine {
                layer: 0,
                interior: false,
            })
            .map(|v| v.rate)
            .expect("layer 0");
        sound.state.rpm = 8000.0;
        let high = build_mix_frame(&sound, &common)
            .voice(VoiceId::Engine {
                layer: 0,
                interior: false,
            })
            .map(|v| v.rate)
            .expect("layer 0");
        assert!(high > low, "rate {high} should exceed {low}");
        for id in [
            VoiceId::Engine {
                layer: 0,
                interior: true,
            },
            VoiceId::Engine {
                layer: 7,
                interior: false,
            },
        ] {
            for rpm in [0.0, 1200.0, 4000.0, 9000.0] {
                sound.state.rpm = rpm;
                let rate = build_mix_frame(&sound, &common)
                    .voice(id)
                    .map(|v| v.rate)
                    .expect("layer");
                assert!((0.5..=2.0).contains(&rate), "rpm {rpm} gave {rate}");
            }
        }
    }

    #[test]
    fn tyre_rates_stay_audible_at_any_speed() {
        for speed in [0.0, 10.0, 60.0, 120.0] {
            let rolling = TyreKind::Rolling.rate(speed);
            let skid = TyreKind::Skid.rate(speed);
            assert!((0.5..=1.5).contains(&rolling), "rolling {rolling}");
            assert!((0.9..=1.6).contains(&skid), "skid {skid}");
        }
        assert!(
            TyreKind::Skid.rate(0.0) >= 0.9,
            "a lock-up is audible at rest"
        );
    }

    #[test]
    fn the_engine_set_weights_crossfade_across_the_band() {
        let mut sound = chase();
        let common = CommonSound::default();
        let band = sound.engine_ext[0].clone();
        let id = VoiceId::Engine {
            layer: 0,
            interior: false,
        };

        // The layer's weight is a triangle: it peaks at its own mid-band...
        sound.state.rpm = (band.rpm_min + band.rpm_max) * 0.5;
        let center = build_mix_frame(&sound, &common).gain(id);
        // ...and falls to nothing at its edge.
        sound.state.rpm = band.rpm_min + 0.5;
        let edge = build_mix_frame(&sound, &common).gain(id);

        assert!(center > 0.0, "the mid-band layer is audible: {center}");
        assert!(center > edge, "center {center} vs edge {edge}");
    }

    #[test]
    fn the_frame_only_changes_with_the_models() {
        let sound = chase();
        let common = CommonSound::default();
        assert_eq!(
            build_mix_frame(&sound, &common),
            build_mix_frame(&sound, &common)
        );
    }
}
