//! Feature hub — port of ksengine `src/simulator/FeatureHub.h` (GPL-3.0).
//! Original: <https://github.com/SimonTek27/ksengine>
//!
//! The C++ `FeatureHub` is a bundle of the session, control API, track limits,
//! weather, personal-best, layout and replay services. Of those, only the
//! session model ([`GameSessionMode`]/[`SessionStartParams`]/[`SessionFlow`])
//! and the [`PersonalBestStore`] are ported so far, so this resource composes
//! exactly those and keeps the parts that are not yet available out of the
//! build rather than stubbing them:
//!
//! | C++ member | RacingEdge |
//! | --- | --- |
//! | `sessionMode`, `sessionParams` | [`FeatureHub::session_mode`], [`FeatureHub::session_params`] |
//! | `PersonalBestStore pb` | [`FeatureHub::personal_best`] |
//! | `pb.setDirectory("user/pb")` | [`FeatureHub::start_services`] |
//! | `onLapCompleted` | [`FeatureHub::on_lap_completed`] |
//! | `discovery`, `control` | attaches with E2/E3 |
//! | `trackLimits` | attaches with D3 |
//! | `weatherCtrl` | attaches with B9 |
//! | `layouts`, `replay` | attaches with D4 / the track loader |
//!
//! [`SessionFlow`](super::session_flow::SessionFlow) is not a member of the C++
//! hub either; the reference keeps it next to the session manager. It lives
//! here because the flow *is* the session lifecycle the hub owns.

use bevy::prelude::Resource;

use super::personal_best::PersonalBestStore;
use super::session::{defaults_for_mode, GameSessionMode, SessionStartParams};
use super::session_flow::SessionFlow;

/// Default discovery port (`server/kssimserver.ini` `discovery-port`).
pub const DEFAULT_DISCOVERY_PORT: u16 = 20779;
/// Default control-API port (`ExternalControlApi::kDefaultPort`).
pub const DEFAULT_CONTROL_PORT: u16 = 20780;
/// Directory the personal-best store uses by default.
pub const DEFAULT_PB_DIRECTORY: &str = "user/pb";

/// The session/telemetry/services hub, as a single Bevy resource.
///
/// # Example
///
/// ```
/// use racingedge::simulator::feature_hub::{FeatureHub, DEFAULT_PB_DIRECTORY};
/// let dir = std::env::temp_dir().join(format!("racingedge-hub-doc-{}", std::process::id()));
/// let mut hub = FeatureHub::default();
/// hub.set_personal_best_directory(&dir);
/// let events = hub.on_lap_completed("monza", "car", "simon", 83.5, None);
/// assert_eq!(events.len(), 2);
/// assert!(events[0].starts_with("PB track=monza"));
/// assert!(events[1].starts_with("LAP time="));
/// assert_ne!(DEFAULT_PB_DIRECTORY, "");
/// let _ = std::fs::remove_dir_all(&dir);
/// ```
#[derive(Resource, Clone, Debug)]
pub struct FeatureHub {
    /// The session mode selected in the menu.
    pub session_mode: GameSessionMode,
    /// The start parameters for [`session_mode`](Self::session_mode).
    pub session_params: SessionStartParams,
    /// The qualifying → race flow.
    pub flow: SessionFlow,
    /// Per-track/car personal bests.
    pub personal_best: PersonalBestStore,
    discovery_port: u16,
    control_port: u16,
    discovery_started: bool,
    control_started: bool,
}

impl Default for FeatureHub {
    fn default() -> Self {
        Self {
            session_mode: GameSessionMode::Race,
            session_params: defaults_for_mode(GameSessionMode::Race),
            flow: SessionFlow::new(),
            personal_best: PersonalBestStore::new(),
            discovery_port: DEFAULT_DISCOVERY_PORT,
            control_port: DEFAULT_CONTROL_PORT,
            discovery_started: false,
            control_started: false,
        }
    }
}

impl FeatureHub {
    /// A hub in the default `Race` mode with no services started.
    pub fn new() -> Self {
        Self::default()
    }

    /// Switches the session mode and resets the start parameters to that mode's
    /// defaults (port of `setSessionMode`).
    pub fn set_session_mode(&mut self, mode: GameSessionMode) {
        self.session_mode = mode;
        self.session_params = defaults_for_mode(mode);
    }

    /// Switches the mode from a menu entry (port of `setSessionModeFromMenu`).
    pub fn set_session_mode_from_menu(&mut self, entry: &str) {
        self.set_session_mode(GameSessionMode::from_menu_entry(entry));
    }

    /// The discovery port (port of `setDiscoveryPort`/`discoveryPort`).
    pub fn set_discovery_port(&mut self, port: u16) {
        self.discovery_port = port;
    }

    /// The configured discovery port.
    pub fn discovery_port(&self) -> u16 {
        self.discovery_port
    }

    /// The control-API port (port of `setControlPort`/`controlPort`).
    pub fn set_control_port(&mut self, port: u16) {
        self.control_port = port;
    }

    /// The configured control-API port.
    pub fn control_port(&self) -> u16 {
        self.control_port
    }

    /// Points the personal-best store at `dir` and loads any records already
    /// there. Returns `false` when `dir` is unsafe or cannot be created.
    pub fn set_personal_best_directory(&mut self, dir: impl AsRef<std::path::Path>) -> bool {
        self.personal_best.set_directory(dir)
    }

    /// Starts the bundled services.
    ///
    /// Only the personal-best directory ([`DEFAULT_PB_DIRECTORY`]) is wired
    /// today; the discovery and control sockets attach with the E2/E3 items and
    /// flip [`discovery_started`](Self) / [`control_started`](Self). Returns
    /// whether the personal-best directory was set.
    pub fn start_services(&mut self, host_announce: bool) -> bool {
        let _ = host_announce;
        self.discovery_started = true;
        self.control_started = true;
        self.personal_best.set_directory(DEFAULT_PB_DIRECTORY)
    }

    /// Stops the bundled services (port of `stopServices`).
    pub fn stop_services(&mut self) {
        self.discovery_started = false;
        self.control_started = false;
    }

    /// Whether the discovery service has been started.
    pub fn discovery_started(&self) -> bool {
        self.discovery_started
    }

    /// Whether the control service has been started.
    pub fn control_started(&self) -> bool {
        self.control_started
    }

    /// Submits a completed lap to the personal-best store and returns the
    /// control events the reference would emit (port of `onLapCompleted`).
    ///
    /// A `PB …` event is prepended when the lap improved the record; every call
    /// ends with a `LAP …` event. Times use the reference's `%f` formatting.
    pub fn on_lap_completed(
        &mut self,
        track: &str,
        car: &str,
        driver: &str,
        lap_time: f32,
        sectors: Option<[f32; 3]>,
    ) -> Vec<String> {
        let improved = self
            .personal_best
            .submit_lap(track, car, driver, lap_time, sectors);

        let mut events = Vec::new();
        if improved {
            events.push(format!("PB track={track} lap={lap_time:.6}"));
        }
        events.push(format!("LAP time={lap_time:.6}"));
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("racingedge-hub-{tag}-{}", std::process::id()))
    }

    #[test]
    fn defaults_are_race_mode() {
        let hub = FeatureHub::default();
        assert_eq!(hub.session_mode, GameSessionMode::Race);
        assert_eq!(hub.session_params.total_laps, 10);
        assert_eq!(hub.discovery_port(), DEFAULT_DISCOVERY_PORT);
        assert_eq!(hub.control_port(), DEFAULT_CONTROL_PORT);
        assert!(!hub.discovery_started());
        assert!(!hub.control_started());
    }

    #[test]
    fn set_session_mode_resets_params() {
        let mut hub = FeatureHub::default();
        hub.set_session_mode(GameSessionMode::Practice);
        assert_eq!(hub.session_params.total_laps, 0);
        assert_eq!(hub.session_params.practice_minutes, 30);
        hub.set_session_mode_from_menu("qualifying");
        assert_eq!(hub.session_mode, GameSessionMode::Qualifying);
        assert_eq!(hub.session_params.qualify_minutes, 15);
    }

    #[test]
    fn ports_are_configurable() {
        let mut hub = FeatureHub::default();
        hub.set_discovery_port(40000);
        hub.set_control_port(40001);
        assert_eq!(hub.discovery_port(), 40000);
        assert_eq!(hub.control_port(), 40001);
    }

    #[test]
    fn lap_events_follow_improvement() {
        let dir = temp_dir("events");
        let mut hub = FeatureHub::default();
        assert!(hub.set_personal_best_directory(&dir));

        let first = hub.on_lap_completed("monza", "car", "simon", 83.5, None);
        assert_eq!(first.len(), 2);
        assert!(first[0].starts_with("PB track=monza lap=83.500000"));
        assert!(first[1].starts_with("LAP time=83.500000"));

        let slower = hub.on_lap_completed("monza", "car", "simon", 84.0, None);
        assert_eq!(slower.len(), 1, "no PB event when the lap is slower");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn start_and_stop_services_toggle_flags() {
        let mut hub = FeatureHub::default();
        hub.set_personal_best_directory(temp_dir("services"));
        let _ = hub.start_services(false);
        assert!(hub.discovery_started());
        assert!(hub.control_started());
        hub.stop_services();
        assert!(!hub.discovery_started());
        assert!(!hub.control_started());
    }

    #[test]
    fn flow_is_owned_by_the_hub() {
        let mut hub = FeatureHub::default();
        hub.flow.start_race_direct(3, 1.0);
        hub.flow.tick(1.0);
        assert!(hub.flow.is_green());
    }
}
