//! Replay runtime wiring (D4) — ties the A7 replay recorder/player
//! (`crate::formats::replay::ReplayRecorder`, a port of ksengine
//! `ReplayRecorder.*`, GPL-3.0) to the deterministic fixed-step loop
//! ([`crate::simulator::r#loop`]) and to the vehicle state.
//!
//! The recorder model and the on-disk format landed with A7; this module is
//! the app-layer glue the backlog called for: a single [`ReplayRuntime`] that
//!
//! * advances a simulation clock by `dt` and, while recording, feeds the
//!   current vehicle snapshot to the recorder. `ReplayRecorder::record_frame`
//!   already throttles to 60 Hz, so calling it every 1 kHz physics step records
//!   at the reference rate;
//! * drives playback (`update_playback`, `playback_frame`) so a renderer can
//!   apply the interpolated frame to the car transform.
//!
//! The Bevy plugin that binds this to the player car's [`VehicleState`] and a
//! camera lives in the app; the runtime itself is pure and unit-tested.

use std::path::Path;

use bevy::prelude::Resource;

use crate::formats::replay::{ReplayFrame, ReplayRecorder, ReplaySample};
use crate::simulator::physics::VehicleState;
use crate::simulator::r#loop::PHYSICS_DT;

/// Bevy resource that records and plays back the player car's replay.
#[derive(Clone, Debug, Default, Resource)]
pub struct ReplayRuntime {
    recorder: ReplayRecorder,
    sim_time: f32,
}

impl ReplayRuntime {
    /// A runtime wrapping a fresh recorder.
    pub fn new() -> Self {
        Self {
            recorder: ReplayRecorder::new(),
            sim_time: 0.0,
        }
    }

    /// Advance the simulation clock by `dt` and, while recording, capture
    /// `sample`. Returns `true` when a frame was actually stored (the recorder
    /// keeps at most 60 Hz, so a 1 kHz call mostly returns `false`).
    pub fn step(&mut self, dt: f32, sample: ReplaySample) -> bool {
        self.sim_time += dt;
        self.recorder.record_frame(self.sim_time, sample)
    }

    /// Convenience: run exactly one fixed physics tick with a vehicle snapshot
    /// built from `state` at world `position`/`rotation`.
    pub fn step_vehicle(
        &mut self,
        state: &VehicleState,
        position: [f32; 3],
        rotation: [f32; 3],
    ) -> bool {
        self.step(PHYSICS_DT as f32, vehicle_sample(state, position, rotation))
    }

    /// Start a fresh recording (clears any previous frames and the clock).
    pub fn start_recording(&mut self) {
        self.sim_time = 0.0;
        self.recorder.start_recording();
    }

    /// Stop recording and return the number of stored frames.
    pub fn stop_recording(&mut self) -> usize {
        self.recorder.stop_recording()
    }

    pub fn is_recording(&self) -> bool {
        self.recorder.is_recording()
    }

    /// Start (or restart) playback from the beginning; `false` with no frames.
    pub fn start_playback(&mut self) -> bool {
        self.recorder.start_playback()
    }

    pub fn stop_playback(&mut self) {
        self.recorder.stop_playback();
    }

    pub fn is_playing(&self) -> bool {
        self.recorder.is_playing()
    }

    /// Advance playback by `dt` (scaled by the playback speed).
    pub fn update_playback(&mut self, dt: f32) {
        self.recorder.update_playback(dt);
    }

    /// The interpolated frame at the current playback time.
    pub fn playback_frame(&self) -> ReplayFrame {
        self.recorder.playback_frame()
    }

    /// Seek to a fraction (`0..=1`) of the recorded clip.
    pub fn seek_progress(&mut self, t01: f32) {
        self.recorder.seek_progress(t01);
    }

    /// Playback progress in `0..=1`.
    pub fn playback_progress(&self) -> f32 {
        self.recorder.playback_progress()
    }

    /// Whether playback reached the end.
    pub fn is_finished(&self) -> bool {
        self.recorder.is_finished()
    }

    pub fn set_playback_speed(&mut self, speed: f32) {
        self.recorder.set_playback_speed(speed);
    }

    /// Recorded clip duration in seconds.
    pub fn duration(&self) -> f32 {
        self.recorder.duration()
    }

    pub fn frame_count(&self) -> usize {
        self.recorder.frame_count()
    }

    /// Simulation clock in seconds (recording time base).
    pub fn sim_time(&self) -> f32 {
        self.sim_time
    }

    pub fn recorder(&self) -> &ReplayRecorder {
        &self.recorder
    }

    pub fn recorder_mut(&mut self) -> &mut ReplayRecorder {
        &mut self.recorder
    }

    /// Persist the recorded clip (port of `saveReplay`).
    pub fn save(&self, path: impl AsRef<Path>) -> bool {
        self.recorder.save_replay(path)
    }

    /// Load a clip into the runtime (port of `loadReplay`). Returns `false`
    /// when the file is missing or corrupted.
    pub fn load(&mut self, path: impl AsRef<Path>) -> bool {
        self.recorder.load_replay(path)
    }
}

/// Build a [`ReplaySample`] from a physics snapshot.
pub fn vehicle_sample(
    state: &VehicleState,
    position: [f32; 3],
    rotation: [f32; 3],
) -> ReplaySample {
    ReplaySample {
        position,
        rotation,
        speed: state.speed,
        rpm: state.engine.rpm,
        gear: state.drivetrain.gear as i32,
        throttle: state.throttle,
        brake: state.brake,
        steering: state.steer_input,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(speed: f32) -> ReplaySample {
        ReplaySample {
            position: [1.0, 0.0, 2.0],
            rotation: [0.0, 0.5, 0.0],
            speed,
            rpm: 4000.0,
            gear: 3,
            throttle: 0.7,
            brake: 0.0,
            steering: 0.2,
        }
    }

    #[test]
    fn new_runtime_is_idle() {
        let rt = ReplayRuntime::new();
        assert!(!rt.is_recording());
        assert!(!rt.is_playing());
        assert_eq!(rt.frame_count(), 0);
        assert_eq!(rt.sim_time(), 0.0);
    }

    #[test]
    fn step_is_a_noop_when_not_recording() {
        let mut rt = ReplayRuntime::new();
        assert!(!rt.step(0.02, sample(10.0)));
        assert_eq!(rt.frame_count(), 0);
        assert!((rt.sim_time() - 0.02).abs() < 1e-6, "clock still advances");
    }

    #[test]
    fn records_at_60hz_from_a_1khz_loop() {
        let mut rt = ReplayRuntime::new();
        rt.start_recording();
        // One simulated second of 1 kHz physics ticks.
        let mut stored = 0;
        for _ in 0..1000 {
            if rt.step(PHYSICS_DT as f32, sample(30.0)) {
                stored += 1;
            }
        }
        // 60 Hz over one second: the first frame plus one per 1/60 s boundary.
        assert!(
            (59..=61).contains(&stored),
            "expected ~60 stored frames, got {stored}"
        );
        assert_eq!(rt.frame_count(), stored);
    }

    #[test]
    fn first_frame_is_recorded_immediately() {
        let mut rt = ReplayRuntime::new();
        rt.start_recording();
        assert!(rt.step(0.001, sample(1.0)));
        assert_eq!(rt.frame_count(), 1);
    }

    #[test]
    fn playback_interpolates_recorded_frames() {
        let mut rt = ReplayRuntime::new();
        rt.start_recording();
        // Two frames exactly 1/60 s apart, with different speeds.
        rt.step(0.0, sample(0.0));
        rt.step(1.0 / 60.0, sample(60.0));
        assert_eq!(rt.frame_count(), 2);

        assert!(rt.start_playback());
        assert!(rt.is_playing());
        rt.update_playback(1.0 / 120.0); // half-way between the two frames
        let frame = rt.playback_frame();
        assert!(
            (frame.speed - 30.0).abs() < 1.0,
            "midway speed should interpolate, got {}",
            frame.speed
        );
    }

    #[test]
    fn playback_reaches_finish() {
        let mut rt = ReplayRuntime::new();
        rt.start_recording();
        rt.step(0.0, sample(0.0));
        rt.step(1.0, sample(10.0));
        rt.start_playback();
        rt.update_playback(5.0);
        assert!(rt.is_finished());
        assert!((rt.playback_progress() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn vehicle_sample_reads_state() {
        let mut state = VehicleState {
            speed: 42.0,
            throttle: 0.9,
            brake: 0.1,
            steer_input: -0.3,
            ..Default::default()
        };
        state.engine.rpm = 7000.0;
        state.drivetrain.gear = 4;
        let s = vehicle_sample(&state, [3.0, 0.0, 4.0], [0.0, 1.0, 0.0]);
        assert_eq!(s.position, [3.0, 0.0, 4.0]);
        assert_eq!(s.rotation, [0.0, 1.0, 0.0]);
        assert_eq!(s.speed, 42.0);
        assert_eq!(s.rpm, 7000.0);
        assert_eq!(s.gear, 4);
        assert_eq!(s.throttle, 0.9);
        assert_eq!(s.brake, 0.1);
        assert_eq!(s.steering, -0.3);
    }

    #[test]
    fn step_vehicle_uses_physics_dt() {
        let mut rt = ReplayRuntime::new();
        rt.start_recording();
        let state = VehicleState::default();
        assert!(rt.step_vehicle(&state, [0.0; 3], [0.0; 3]));
        assert!((rt.sim_time() - PHYSICS_DT as f32).abs() < 1e-9);
    }

    #[test]
    fn start_recording_resets_clock_and_frames() {
        let mut rt = ReplayRuntime::new();
        rt.start_recording();
        rt.step(0.0, sample(0.0));
        rt.step(1.0, sample(1.0));
        assert!(rt.frame_count() > 0);
        rt.start_recording();
        assert_eq!(rt.frame_count(), 0);
        assert_eq!(rt.sim_time(), 0.0);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let mut rt = ReplayRuntime::new();
        rt.start_recording();
        rt.step(0.0, sample(5.0));
        rt.step(1.0, sample(50.0));
        rt.stop_recording();

        let dir = std::env::temp_dir().join("racingedge_replay_runtime_test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("clip.rea.replay");
        assert!(rt.save(&path), "save should succeed");

        let mut loaded = ReplayRuntime::new();
        assert!(loaded.load(&path), "load should succeed");
        assert_eq!(loaded.frame_count(), rt.frame_count());
        let _ = std::fs::remove_file(&path);
    }
}
