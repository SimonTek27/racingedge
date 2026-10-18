//! C ABI for headless hosts — port of ksengine `include/ksengine_c.h`.
//!
//! The engine is a single fixed-step vehicle plus a session authority, so an
//! external host (a dedicated server, a GDExtension, a test harness) can drive
//! it without linking Rust. Every entry point is `extern "C"`, takes the opaque
//! [`ReEngine`] handle by pointer and is null-safe: a null handle is a no-op
//! except where a value must be returned, where a neutral value is produced.
//!
//! The matching hand-written header lives in `include/racingedge.h`; the
//! [`EXPORTED_SYMBOLS`] list is the source of truth and a unit test keeps the
//! two in sync. No code generator (and therefore no extra dependency) is used.
//!
//! ```
//! use racingedge::c_api::{
//!     re_engine_create, re_engine_destroy, re_engine_get_state, re_engine_set_input,
//!     re_engine_step, ReInput, ReVehicleState,
//! };
//!
//! let eng = re_engine_create(0);
//! let input = ReInput { throttle: 1.0, ..ReInput::default() };
//! re_engine_set_input(eng, &input);
//! re_engine_step(eng, 1.0);
//!
//! let mut state = ReVehicleState::default();
//! re_engine_get_state(eng, &mut state);
//! assert!(state.speed_ms > 0.0);
//! re_engine_destroy(eng);
//! ```

#![allow(clippy::not_unsafe_ptr_arg_deref)]

use core::ffi::c_char;

use bevy::prelude::Transform;

use crate::simulator::physics::{integrate_transform, step_vehicle, VehicleConfig, VehicleState};
use crate::simulator::session_flow::{SessionFlow, SessionFlowPhase};

/// Default fixed physics step, matching the reference `0.001 s`.
const DEFAULT_TIMESTEP: f64 = 0.001;

/// Every `#[no_mangle]` symbol this module exports, kept in alphabetical order.
/// The header `include/racingedge.h` must declare all of them.
pub const EXPORTED_SYMBOLS: &[&str] = &[
    "re_engine_create",
    "re_engine_destroy",
    "re_engine_get_state",
    "re_engine_reset",
    "re_engine_session_configure",
    "re_engine_session_lap",
    "re_engine_session_phase",
    "re_engine_session_remaining",
    "re_engine_session_start",
    "re_engine_session_time",
    "re_engine_set_input",
    "re_engine_set_replay_path",
    "re_engine_set_timestep",
    "re_engine_step",
    "re_engine_version",
];

/// Primary vehicle state read back by [`re_engine_get_state`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ReVehicleState {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub yaw: f32,
    pub pitch: f32,
    pub roll: f32,
    pub speed_ms: f32,
    pub rpm: f32,
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub fuel_l: f32,
    pub tyre_temp_fl: f32,
    pub tyre_temp_fr: f32,
    pub tyre_temp_rl: f32,
    pub tyre_temp_rr: f32,
}

/// Driver input applied by [`re_engine_set_input`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ReInput {
    /// `0..1`
    pub throttle: f32,
    pub brake: f32,
    /// `-1..1`
    pub steer: f32,
    pub clutch: f32,
    /// `0` = no request
    pub gear_request: i32,
}

/// Session parameters applied by [`re_engine_session_configure`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct ReSessionConfig {
    pub total_laps: i32,
    pub time_limit_s: i32,
    pub countdown_s: f32,
    pub track_length_m: f32,
}

/// Session phase reported by [`re_engine_session_phase`].
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReSessionPhase {
    Idle = 0,
    Countdown = 1,
    Green = 2,
    Checkered = 3,
}

impl From<SessionFlowPhase> for ReSessionPhase {
    fn from(phase: SessionFlowPhase) -> Self {
        match phase {
            SessionFlowPhase::Countdown => ReSessionPhase::Countdown,
            SessionFlowPhase::Race => ReSessionPhase::Green,
            SessionFlowPhase::Finished => ReSessionPhase::Checkered,
            SessionFlowPhase::Idle
            | SessionFlowPhase::Qualifying
            | SessionFlowPhase::GridPublished => ReSessionPhase::Idle,
        }
    }
}

/// The host-visible engine instance behind the opaque `ReEngine*` handle.
pub struct ReEngine {
    cfg: VehicleConfig,
    car: VehicleState,
    transform: Transform,
    timestep: f64,
    accumulator: f64,
    input: ReInput,
    session: SessionFlow,
    session_cfg: ReSessionConfig,
    elapsed: f64,
    distance: f64,
    lap: i32,
}

impl ReEngine {
    fn new() -> Self {
        let countdown = 5.0;
        let mut session = SessionFlow::default();
        session.start_race_direct(0, countdown);
        Self {
            cfg: VehicleConfig::default(),
            car: VehicleState::default(),
            transform: Transform::default(),
            timestep: DEFAULT_TIMESTEP,
            accumulator: 0.0,
            input: ReInput::default(),
            session,
            session_cfg: ReSessionConfig {
                total_laps: 0,
                time_limit_s: 0,
                countdown_s: countdown,
                track_length_m: 0.0,
            },
            elapsed: 0.0,
            distance: 0.0,
            lap: 0,
        }
    }

    fn step_fixed(&mut self, dt: f32) {
        self.car.throttle = self.input.throttle.clamp(0.0, 1.0);
        self.car.brake = self.input.brake.clamp(0.0, 1.0);
        self.car.steer_input = self.input.steer.clamp(-1.0, 1.0);
        if self.input.gear_request > 0 {
            self.car.drivetrain.gear = self.input.gear_request as i8;
        }
        step_vehicle(&self.cfg, &mut self.car, dt);
        integrate_transform(&self.car, &mut self.transform, dt);

        self.session.tick(dt);
        if let Some(limit) = self.time_limit() {
            if self.elapsed >= limit {
                self.session.finish_race(Vec::new());
            }
        }
        if self.session.phase() == SessionFlowPhase::Race {
            self.distance += (self.car.speed * dt) as f64;
            let length = self.session_cfg.track_length_m as f64;
            if length > 0.0 {
                let lap = (self.distance / length).floor() as i32 + 1;
                self.lap = lap.max(1);
                let total = self.session_cfg.total_laps;
                if total > 0 && self.lap > total {
                    self.session.finish_race(Vec::new());
                }
            } else if self.lap == 0 {
                self.lap = 1;
            }
        }
    }

    fn time_limit(&self) -> Option<f64> {
        (self.session_cfg.time_limit_s > 0).then_some(self.session_cfg.time_limit_s as f64)
    }
}

fn engine<'a>(eng: *mut ReEngine) -> Option<&'a mut ReEngine> {
    // SAFETY: the caller owns the pointer, which was either produced by
    // `re_engine_create` or is null (handled here).
    unsafe { eng.as_mut() }
}

/// Create an engine instance. `seed` is reserved for deterministic runs; the
/// current implementation is already deterministic. Returns null on allocation
/// failure only.
///
/// # Safety
/// The returned pointer must be released with [`re_engine_destroy`].
#[no_mangle]
pub extern "C" fn re_engine_create(_seed: u32) -> *mut ReEngine {
    Box::into_raw(Box::new(ReEngine::new()))
}

/// Destroy an engine instance. Passing null is a no-op.
///
/// # Safety
/// `eng` must have been returned by [`re_engine_create`] and not destroyed.
#[no_mangle]
pub extern "C" fn re_engine_destroy(eng: *mut ReEngine) {
    if !eng.is_null() {
        // SAFETY: ownership was transferred to us by `re_engine_create`.
        drop(unsafe { Box::from_raw(eng) });
    }
}

/// Fixed physics step in seconds (default `0.001`). Non-positive values are
/// ignored.
///
/// # Safety
/// `eng` is an engine handle or null.
#[no_mangle]
pub extern "C" fn re_engine_set_timestep(eng: *mut ReEngine, dt: f64) {
    if let Some(e) = engine(eng) {
        if dt.is_finite() && dt > 0.0 {
            e.timestep = dt;
        }
    }
}

/// Reset the vehicle and session to their initial state, keeping the timestep.
///
/// # Safety
/// `eng` is an engine handle or null.
#[no_mangle]
pub extern "C" fn re_engine_reset(eng: *mut ReEngine) {
    if let Some(e) = engine(eng) {
        let timestep = e.timestep;
        let session_cfg = e.session_cfg;
        *e = ReEngine::new();
        e.timestep = timestep;
        e.session_cfg = session_cfg;
        e.apply_session_config();
    }
}

/// Apply driver input for subsequent steps.
///
/// # Safety
/// `eng` is an engine handle or null; `input` is a valid pointer or null.
#[no_mangle]
pub extern "C" fn re_engine_set_input(eng: *mut ReEngine, input: *const ReInput) {
    if let (Some(e), Some(input)) = (engine(eng), unsafe { input.as_ref() }) {
        e.input = *input;
    }
}

/// Advance the simulation by `dt` seconds, subdividing into fixed steps.
///
/// # Safety
/// `eng` is an engine handle or null.
#[no_mangle]
pub extern "C" fn re_engine_step(eng: *mut ReEngine, dt: f64) {
    let Some(e) = engine(eng) else { return };
    if !dt.is_finite() || dt <= 0.0 {
        return;
    }
    e.accumulator += dt;
    let mut guard = 0;
    while e.accumulator >= e.timestep && guard < 100_000 {
        let step = e.timestep as f32;
        e.step_fixed(step);
        e.elapsed += e.timestep;
        e.accumulator -= e.timestep;
        guard += 1;
    }
}

/// Read the primary vehicle state. A null `out` is a no-op.
///
/// # Safety
/// `eng` is an engine handle or null; `out` is a valid pointer or null.
#[no_mangle]
pub extern "C" fn re_engine_get_state(eng: *const ReEngine, out: *mut ReVehicleState) {
    let Some(out) = (unsafe { out.as_mut() }) else {
        return;
    };
    let Some(e) = (unsafe { eng.as_ref() }) else {
        *out = ReVehicleState::default();
        return;
    };
    let p = e.transform.translation;
    *out = ReVehicleState {
        x: p.x,
        y: p.y,
        z: p.z,
        yaw: e.car.yaw,
        pitch: 0.0,
        roll: 0.0,
        speed_ms: e.car.speed,
        rpm: e.car.engine.rpm,
        gear: e.car.drivetrain.gear as i32,
        throttle: e.car.throttle,
        brake: e.car.brake,
        steer: e.car.steer_input,
        fuel_l: 0.0,
        tyre_temp_fl: 0.0,
        tyre_temp_fr: 0.0,
        tyre_temp_rl: 0.0,
        tyre_temp_rr: 0.0,
    };
}

/// Version string `"major.minor.patch"`, static for the process lifetime.
#[no_mangle]
pub extern "C" fn re_engine_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

/// Enable binary replay recording to `path_utf8` (empty stops). Returns non-zero
/// on success; `0` means unsupported: the per-engine replay bridge is not wired
/// yet, so hosts must not rely on recording from the C ABI.
///
/// # Safety
/// `eng` is an engine handle or null; `path_utf8` is a NUL-terminated string or
/// null.
#[no_mangle]
pub extern "C" fn re_engine_set_replay_path(_eng: *mut ReEngine, _path_utf8: *const c_char) -> i32 {
    0
}

/// Configure the session. Call before [`re_engine_session_start`].
///
/// # Safety
/// `eng` is an engine handle or null; `cfg` is a valid pointer or null.
#[no_mangle]
pub extern "C" fn re_engine_session_configure(eng: *mut ReEngine, cfg: *const ReSessionConfig) {
    if let (Some(e), Some(cfg)) = (engine(eng), unsafe { cfg.as_ref() }) {
        e.session_cfg = *cfg;
        e.apply_session_config();
    }
}

/// Start the configured session (enters the countdown).
///
/// # Safety
/// `eng` is an engine handle or null.
#[no_mangle]
pub extern "C" fn re_engine_session_start(eng: *mut ReEngine) {
    if let Some(e) = engine(eng) {
        e.apply_session_config();
        e.elapsed = 0.0;
        e.distance = 0.0;
        e.lap = 0;
    }
}

/// Session phase.
///
/// # Safety
/// `eng` is an engine handle or null.
#[no_mangle]
pub extern "C" fn re_engine_session_phase(eng: *const ReEngine) -> ReSessionPhase {
    match unsafe { eng.as_ref() } {
        Some(e) => e.session.phase().into(),
        None => ReSessionPhase::Idle,
    }
}

/// Current lap (1-based) while green; `0` before the race starts.
///
/// # Safety
/// `eng` is an engine handle or null.
#[no_mangle]
pub extern "C" fn re_engine_session_lap(eng: *const ReEngine) -> i32 {
    unsafe { eng.as_ref() }.map_or(0, |e| e.lap)
}

/// Seconds elapsed since the session started.
///
/// # Safety
/// `eng` is an engine handle or null.
#[no_mangle]
pub extern "C" fn re_engine_session_time(eng: *const ReEngine) -> f64 {
    unsafe { eng.as_ref() }.map_or(0.0, |e| e.elapsed)
}

/// Seconds left on the time limit, or `-1` when the session has none.
///
/// # Safety
/// `eng` is an engine handle or null.
#[no_mangle]
pub extern "C" fn re_engine_session_remaining(eng: *const ReEngine) -> f64 {
    match unsafe { eng.as_ref() } {
        Some(e) => match e.time_limit() {
            Some(limit) => (limit - e.elapsed).max(0.0),
            None => -1.0,
        },
        None => -1.0,
    }
}

impl ReEngine {
    fn apply_session_config(&mut self) {
        let cfg = self.session_cfg;
        self.session = SessionFlow::default();
        self.session
            .start_race_direct(cfg.total_laps, cfg.countdown_s);
    }
}

/// Fetch a NUL-terminated UTF-8 string as a `&str`, borrowed from the caller.
///
/// Only used by tests today; kept `pub` so hosts embedding the crate can reuse
/// the same conversion.
///
/// # Safety
/// `ptr` must be NUL-terminated and valid for the borrow, or null.
pub unsafe fn cstr(ptr: *const c_char) -> Option<&'static str> {
    if ptr.is_null() {
        return None;
    }
    let c = unsafe { std::ffi::CStr::from_ptr(ptr) };
    c.to_str().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_path() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("include/racingedge.h")
    }

    #[test]
    fn version_is_static_and_matches_crate() {
        let v = unsafe { cstr(re_engine_version()) }.unwrap();
        assert_eq!(v, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn create_step_and_read_state() {
        let eng = re_engine_create(0);
        assert!(!eng.is_null());
        re_engine_set_timestep(eng, 0.01);
        let input = ReInput {
            throttle: 1.0,
            ..ReInput::default()
        };
        re_engine_set_input(eng, &input);
        re_engine_step(eng, 2.0);

        let mut st = ReVehicleState::default();
        re_engine_get_state(eng, &mut st);
        assert!(st.speed_ms > 0.0, "car should have moved: {st:?}");
        assert!(st.throttle > 0.9);
        assert!(st.x.abs() + st.z.abs() > 0.0);
        re_engine_destroy(eng);
    }

    #[test]
    fn null_handles_are_safe() {
        re_engine_set_timestep(std::ptr::null_mut(), 0.01);
        re_engine_set_input(std::ptr::null_mut(), std::ptr::null());
        re_engine_step(std::ptr::null_mut(), 0.1);
        re_engine_session_start(std::ptr::null_mut());
        re_engine_destroy(std::ptr::null_mut());

        let mut st = ReVehicleState {
            speed_ms: 7.0,
            ..ReVehicleState::default()
        };
        re_engine_get_state(std::ptr::null(), &mut st);
        assert_eq!(st.speed_ms, 0.0, "null read writes a neutral state");

        assert_eq!(
            re_engine_session_phase(std::ptr::null()),
            ReSessionPhase::Idle
        );
        assert_eq!(re_engine_session_lap(std::ptr::null()), 0);
        assert_eq!(re_engine_session_remaining(std::ptr::null()), -1.0);
        assert_eq!(
            re_engine_set_replay_path(std::ptr::null_mut(), std::ptr::null()),
            0
        );
    }

    #[test]
    fn session_runs_countdown_then_green_and_counts_laps() {
        let eng = re_engine_create(0);
        let cfg = ReSessionConfig {
            total_laps: 3,
            time_limit_s: 0,
            countdown_s: 1.0,
            track_length_m: 10.0,
        };
        re_engine_session_configure(eng, &cfg);
        re_engine_session_start(eng);
        assert_eq!(re_engine_session_phase(eng), ReSessionPhase::Countdown);

        let input = ReInput {
            throttle: 1.0,
            ..ReInput::default()
        };
        re_engine_set_input(eng, &input);
        re_engine_step(eng, 1.2);
        assert_eq!(re_engine_session_phase(eng), ReSessionPhase::Green);
        assert_eq!(re_engine_session_lap(eng), 1);

        re_engine_step(eng, 5.0);
        assert!(re_engine_session_lap(eng) >= 2);
        assert_eq!(re_engine_session_phase(eng), ReSessionPhase::Checkered);
        assert!(re_engine_session_time(eng) >= 1.2);
        re_engine_destroy(eng);
    }

    #[test]
    fn time_limited_session_reports_remaining() {
        let eng = re_engine_create(0);
        let cfg = ReSessionConfig {
            time_limit_s: 2,
            countdown_s: 1.0,
            ..ReSessionConfig::default()
        };
        re_engine_session_configure(eng, &cfg);
        re_engine_session_start(eng);
        re_engine_step(eng, 0.5);
        assert!((re_engine_session_remaining(eng) - 1.5).abs() < 0.01);
        re_engine_step(eng, 3.0);
        assert_eq!(re_engine_session_remaining(eng), 0.0);
        assert_eq!(re_engine_session_phase(eng), ReSessionPhase::Checkered);

        let unlimited = re_engine_create(0);
        assert_eq!(re_engine_session_remaining(unlimited), -1.0);
        re_engine_destroy(unlimited);
        re_engine_destroy(eng);
    }

    #[test]
    fn session_config_is_clamped_to_one_lap() {
        let eng = re_engine_create(0);
        let cfg = ReSessionConfig {
            total_laps: 0,
            countdown_s: 0.0,
            ..ReSessionConfig::default()
        };
        re_engine_session_configure(eng, &cfg);
        assert_eq!(eng_ref(eng).session.total_laps(), 1);
        re_engine_destroy(eng);
    }

    #[test]
    fn header_declares_every_exported_symbol() {
        let header = std::fs::read_to_string(header_path()).expect("include/racingedge.h");
        for symbol in EXPORTED_SYMBOLS {
            assert!(
                header.contains(symbol),
                "include/racingedge.h is missing `{symbol}`"
            );
        }
    }

    #[test]
    fn exported_symbols_list_is_sorted_and_unique() {
        let mut sorted = EXPORTED_SYMBOLS.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, EXPORTED_SYMBOLS, "keep the list sorted and unique");
    }

    #[test]
    fn header_exposes_the_matching_types() {
        let header = std::fs::read_to_string(header_path()).expect("include/racingedge.h");
        for ty in [
            "ReEngine",
            "ReVehicleState",
            "ReInput",
            "ReSessionConfig",
            "ReSessionPhase",
        ] {
            assert!(header.contains(ty), "header is missing type `{ty}`");
        }
    }

    fn eng_ref(eng: *mut ReEngine) -> &'static ReEngine {
        // SAFETY: the test owns the live handle and does not mutate it here.
        unsafe { &*eng }
    }
}
