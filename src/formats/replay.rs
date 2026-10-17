//! Replay — registrazione e riproduzione (port di ksengine `ReplayRecorder.*`).
//!
//! Formato binario little-endian:
//! ```text
//! uint32 magic   = 0x52504C59 ("RPLY")
//! uint32 version = 1
//! uint32 frameCount
//! frameCount * {
//!   float time
//!   float x, y, z            (posizione)
//!   float rx, ry, rz         (rotazione)
//!   float speed, rpm
//!   int32 gear
//!   float throttle, brake, steering
//! }                          (52 byte per frame)
//! ```
//!
//! Il recorder registra a ~60 Hz (scarta i frame più ravvicinati di 1/60 s) e la
//! riproduzione interpola linearmente tra due frame adiacenti (la marcia è uno
//! snap al frame più vicino, con soglia 0.5).

use std::path::Path;

/// Magic di un file replay (`RPLY`).
pub const REPLAY_MAGIC: u32 = 0x5250_4C59;
/// Versione del formato.
pub const REPLAY_VERSION: u32 = 1;
/// Byte di un frame registrato.
pub const FRAME_BYTES: usize = 52;
/// Intervallo minimo tra due frame registrati (60 Hz).
pub const MIN_FRAME_DT: f32 = 1.0 / 60.0;

/// Un fotogramma di replay.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReplayFrame {
    pub time: f32,
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub speed: f32,
    pub rpm: f32,
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub steering: f32,
}

/// I campioni forniti al recorder in un frame (posizione/orientamento + input).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ReplaySample {
    pub position: [f32; 3],
    pub rotation: [f32; 3],
    pub speed: f32,
    pub rpm: f32,
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub steering: f32,
}

/// Registratore/lettore di replay (port di `ReplayRecorder`).
#[derive(Clone, Debug, Default)]
pub struct ReplayRecorder {
    frames: Vec<ReplayFrame>,
    recording: bool,
    playing: bool,
    playback_time: f32,
    playback_speed: f32,
}

impl ReplayRecorder {
    pub fn new() -> Self {
        Self {
            playback_speed: 1.0,
            ..Default::default()
        }
    }

    /// Inizia una registrazione (azzera i frame).
    pub fn start_recording(&mut self) {
        self.frames.clear();
        self.recording = true;
    }

    /// Ferma la registrazione e ritorna il numero di frame raccolti.
    pub fn stop_recording(&mut self) -> usize {
        self.recording = false;
        self.frames.len()
    }

    pub fn is_recording(&self) -> bool {
        self.recording
    }

    /// Registra un frame se il tempo `time` dista almeno 1/60 s dall'ultimo.
    pub fn record_frame(&mut self, time: f32, sample: ReplaySample) -> bool {
        if !self.recording {
            return false;
        }
        if let Some(last) = self.frames.last() {
            if time - last.time < MIN_FRAME_DT {
                return false;
            }
        }
        self.frames.push(ReplayFrame {
            time,
            position: sample.position,
            rotation: sample.rotation,
            speed: sample.speed,
            rpm: sample.rpm,
            gear: sample.gear,
            throttle: sample.throttle,
            brake: sample.brake,
            steering: sample.steering,
        });
        true
    }

    pub fn frames(&self) -> &[ReplayFrame] {
        &self.frames
    }

    pub fn frame_count(&self) -> usize {
        self.frames.len()
    }

    /// Durata coperta dai frame (0 con meno di due frame).
    pub fn duration(&self) -> f32 {
        if self.frames.len() < 2 {
            return 0.0;
        }
        self.frames[self.frames.len() - 1].time - self.frames[0].time
    }

    // ── Serializzazione ──────────────────────────────────────────────────────

    /// Serializza in little-endian (senza disco).
    pub fn to_vec(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + self.frames.len() * FRAME_BYTES);
        out.extend_from_slice(&REPLAY_MAGIC.to_le_bytes());
        out.extend_from_slice(&REPLAY_VERSION.to_le_bytes());
        out.extend_from_slice(&(self.frames.len() as u32).to_le_bytes());
        for fr in &self.frames {
            out.extend_from_slice(&fr.time.to_le_bytes());
            for v in fr.position {
                out.extend_from_slice(&v.to_le_bytes());
            }
            for v in fr.rotation {
                out.extend_from_slice(&v.to_le_bytes());
            }
            out.extend_from_slice(&fr.speed.to_le_bytes());
            out.extend_from_slice(&fr.rpm.to_le_bytes());
            out.extend_from_slice(&fr.gear.to_le_bytes());
            out.extend_from_slice(&fr.throttle.to_le_bytes());
            out.extend_from_slice(&fr.brake.to_le_bytes());
            out.extend_from_slice(&fr.steering.to_le_bytes());
        }
        out
    }

    /// Parsia un replay in memoria.
    pub fn from_bytes(bytes: &[u8]) -> Result<ReplayRecorder, String> {
        if bytes.len() < 12 {
            return Err("unexpected end of file".into());
        }
        let magic = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        if magic != REPLAY_MAGIC {
            return Err(format!(
                "bad replay magic 0x{magic:08x} (expected 0x{REPLAY_MAGIC:08x})"
            ));
        }
        let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        if version != REPLAY_VERSION {
            return Err(format!("unsupported replay version {version}"));
        }
        let count = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        let needed = 12 + count * FRAME_BYTES;
        if bytes.len() < needed {
            return Err("frame data extends past end of file".into());
        }

        let mut frames = Vec::with_capacity(count);
        for i in 0..count {
            let b = 12 + i * FRAME_BYTES;
            let f = |o: usize| f32::from_le_bytes(bytes[b + o..b + o + 4].try_into().unwrap());
            frames.push(ReplayFrame {
                time: f(0),
                position: [f(4), f(8), f(12)],
                rotation: [f(16), f(20), f(24)],
                speed: f(28),
                rpm: f(32),
                gear: i32::from_le_bytes(bytes[b + 36..b + 40].try_into().unwrap()),
                throttle: f(40),
                brake: f(44),
                steering: f(48),
            });
        }
        Ok(ReplayRecorder {
            frames,
            recording: false,
            playing: false,
            playback_time: 0.0,
            playback_speed: 1.0,
        })
    }

    /// Scrive il replay su disco (port di `saveReplay`).
    pub fn save_replay(&self, path: impl AsRef<Path>) -> bool {
        std::fs::write(path, self.to_vec()).is_ok()
    }

    /// Carica un replay da disco (port di `loadReplay`).
    pub fn load_replay(&mut self, path: impl AsRef<Path>) -> bool {
        match std::fs::read(path) {
            Ok(bytes) => match ReplayRecorder::from_bytes(&bytes) {
                Ok(rec) => {
                    self.frames = rec.frames;
                    true
                }
                Err(_) => false,
            },
            Err(_) => false,
        }
    }

    // ── Riproduzione ─────────────────────────────────────────────────────────

    /// Avvia la riproduzione dall'inizio; `false` se non ci sono frame.
    pub fn start_playback(&mut self) -> bool {
        if self.frames.is_empty() {
            return false;
        }
        self.playing = true;
        self.playback_time = 0.0;
        true
    }

    pub fn stop_playback(&mut self) {
        self.playing = false;
        self.playback_time = 0.0;
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn playback_time(&self) -> f32 {
        self.playback_time
    }

    /// Imposta la velocità di riproduzione (clamp 0..8, port di `setPlaybackSpeed`).
    pub fn set_playback_speed(&mut self, speed: f32) {
        self.playback_speed = speed.clamp(0.0, 8.0);
    }

    pub fn playback_speed(&self) -> f32 {
        self.playback_speed
    }

    /// Avanza la riproduzione (port di `updatePlayback`).
    pub fn update_playback(&mut self, dt: f32) {
        if !self.playing {
            return;
        }
        self.playback_time += dt * self.playback_speed;
        let dur = self.duration();
        if dur > 0.0 && self.playback_time > dur {
            self.playback_time = dur;
        }
    }

    /// Raggiunto la fine della riproduzione.
    pub fn is_finished(&self) -> bool {
        !self.frames.is_empty() && self.playback_time >= self.duration() && self.duration() > 0.0
    }

    /// Porta il tempo di riproduzione a `time_sec` (clamp 0..durata).
    pub fn seek_to(&mut self, time_sec: f32) {
        self.playback_time = time_sec.clamp(0.0, self.duration().max(0.0));
    }

    /// Porta la riproduzione alla frazione `t01` in [0, 1].
    pub fn seek_progress(&mut self, t01: f32) {
        self.seek_to(t01.clamp(0.0, 1.0) * self.duration());
    }

    /// Avanzamento della riproduzione in [0, 1] (port di `playbackProgress`).
    pub fn playback_progress(&self) -> f32 {
        let dur = self.duration();
        if dur > 0.0 {
            self.playback_time / dur
        } else {
            0.0
        }
    }

    /// Frame interpolato al tempo `time`.
    pub fn interpolate(&self, time: f32) -> ReplayFrame {
        if self.frames.is_empty() {
            return ReplayFrame::default();
        }
        let mut idx = 0usize;
        while idx + 1 < self.frames.len() && self.frames[idx + 1].time <= time {
            idx += 1;
        }
        if idx + 1 >= self.frames.len() {
            return self.frames[self.frames.len() - 1];
        }
        let a = self.frames[idx];
        let b = self.frames[idx + 1];
        let dt = b.time - a.time;
        let t = if dt > 0.0001 {
            ((time - a.time) / dt).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let lerp = |x: f32, y: f32| x + (y - x) * t;
        let lerp3 =
            |x: [f32; 3], y: [f32; 3]| [lerp(x[0], y[0]), lerp(x[1], y[1]), lerp(x[2], y[2])];
        ReplayFrame {
            time,
            position: lerp3(a.position, b.position),
            rotation: lerp3(a.rotation, b.rotation),
            speed: lerp(a.speed, b.speed),
            rpm: lerp(a.rpm, b.rpm),
            gear: if t < 0.5 { a.gear } else { b.gear },
            throttle: lerp(a.throttle, b.throttle),
            brake: lerp(a.brake, b.brake),
            steering: lerp(a.steering, b.steering),
        }
    }

    /// Frame corrente della riproduzione (port di `getPlaybackFrame`).
    pub fn playback_frame(&self) -> ReplayFrame {
        if self.frames.is_empty() || !self.playing {
            return ReplayFrame::default();
        }
        self.interpolate(self.playback_time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(t: f32) -> ReplaySample {
        ReplaySample {
            position: [t, t * 2.0, 0.0],
            rotation: [0.0, t, 0.0],
            speed: 10.0 + t,
            rpm: 3000.0 + t * 10.0,
            gear: 3,
            throttle: 0.8,
            brake: 0.0,
            steering: 0.1 * t,
        }
    }

    fn recorded() -> ReplayRecorder {
        let mut rec = ReplayRecorder::new();
        rec.start_recording();
        for i in 0..6 {
            rec.record_frame(i as f32 * 0.1, sample(i as f32));
        }
        rec.stop_recording();
        rec
    }

    #[test]
    fn records_at_sixty_hertz_floor() {
        let mut rec = ReplayRecorder::new();
        rec.start_recording();
        assert!(rec.record_frame(0.0, sample(0.0)));
        // Too soon.
        assert!(!rec.record_frame(0.005, sample(1.0)));
        assert!(rec.record_frame(0.02, sample(2.0)));
        assert_eq!(rec.frame_count(), 2);
        assert!(rec.is_recording());
        assert_eq!(rec.stop_recording(), 2);
        assert!(!rec.is_recording());
    }

    #[test]
    fn record_outside_recording_is_ignored() {
        let mut rec = ReplayRecorder::new();
        assert!(!rec.record_frame(0.0, sample(0.0)));
        assert_eq!(rec.frame_count(), 0);
    }

    #[test]
    fn round_trip_preserves_frames() {
        let rec = recorded();
        let bytes = rec.to_vec();
        assert_eq!(&bytes[0..4], &REPLAY_MAGIC.to_le_bytes());
        let back = ReplayRecorder::from_bytes(&bytes).expect("parses");
        assert_eq!(back.frame_count(), 6);
        assert_eq!(back.frames()[3].position, [3.0, 6.0, 0.0]);
        assert_eq!(back.frames()[3].gear, 3);
        assert!((back.frames()[5].steering - 0.5).abs() < 1e-6);
    }

    #[test]
    fn bad_magic_and_version_are_rejected() {
        let mut bytes = recorded().to_vec();
        bytes[0] ^= 0xff;
        assert!(ReplayRecorder::from_bytes(&bytes)
            .unwrap_err()
            .contains("magic"));

        let mut bytes = recorded().to_vec();
        bytes[4..8].copy_from_slice(&9u32.to_le_bytes());
        assert!(ReplayRecorder::from_bytes(&bytes)
            .unwrap_err()
            .contains("unsupported replay version"));
    }

    #[test]
    fn truncated_frame_data_is_rejected() {
        let bytes = recorded().to_vec();
        let err = ReplayRecorder::from_bytes(&bytes[..bytes.len() - 3]).unwrap_err();
        assert!(err.contains("past end of file"), "{err}");
    }

    #[test]
    fn duration_and_progress() {
        let rec = recorded();
        assert!((rec.duration() - 0.5).abs() < 1e-6);
        let mut rec = rec;
        rec.start_playback();
        rec.update_playback(0.25);
        assert!((rec.playback_time() - 0.25).abs() < 1e-6);
        assert!((rec.playback_progress() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn playback_speed_scales_time_and_is_clamped() {
        let mut rec = recorded();
        rec.set_playback_speed(2.0);
        assert_eq!(rec.playback_speed(), 2.0);
        rec.set_playback_speed(99.0);
        assert_eq!(rec.playback_speed(), 8.0);
        rec.set_playback_speed(-1.0);
        assert_eq!(rec.playback_speed(), 0.0);
    }

    #[test]
    fn interpolation_midpoint() {
        let rec = recorded();
        // Between frame 0 (t=0.0) and frame 1 (t=0.1).
        let mid = rec.interpolate(0.05);
        assert!((mid.position[0] - 0.5).abs() < 1e-6);
        assert!((mid.speed - 10.5).abs() < 1e-6);
        // Gear snaps: t<0.5 keeps the left frame's gear.
        assert_eq!(mid.gear, 3);
    }

    #[test]
    fn playback_frame_requires_playing() {
        let rec = recorded();
        assert_eq!(rec.playback_frame(), ReplayFrame::default());
        let mut rec = rec;
        assert!(rec.start_playback());
        rec.update_playback(0.05);
        let f = rec.playback_frame();
        assert!((f.position[0] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn seek_and_finish() {
        let mut rec = recorded();
        rec.start_playback();
        rec.seek_to(100.0);
        assert!((rec.playback_time() - rec.duration()).abs() < 1e-6);
        assert!(rec.is_finished());
        rec.seek_progress(0.0);
        assert_eq!(rec.playback_time(), 0.0);
        assert!(!rec.is_finished());
    }

    #[test]
    fn empty_replay_cannot_play() {
        let mut rec = ReplayRecorder::new();
        assert!(!rec.start_playback());
        assert!(!rec.is_playing());
    }

    #[test]
    fn save_and_load_file() {
        let path = std::env::temp_dir().join("racingedge_replay.rpl");
        let rec = recorded();
        assert!(rec.save_replay(&path));
        let mut loaded = ReplayRecorder::new();
        assert!(loaded.load_replay(&path));
        assert_eq!(loaded.frame_count(), 6);
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_missing_file_fails() {
        let missing = std::env::temp_dir().join("racingedge_replay_absent.rpl");
        let mut rec = ReplayRecorder::new();
        assert!(!rec.load_replay(&missing));
    }
}
