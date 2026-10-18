//! Headless dedicated server (port of ksengine `SimulatorServerApp.cpp` and
//! `tools/ks_server/main.cpp`).
//!
//! The server owns no renderer: it drives the simulation with a built-in pace
//! driver, hosts LAN discovery (`:20779`) and the control API (`:20780`), and
//! runs an optional field of AI cars through the [`MultiCarManager`]. It is a
//! library type so the whole session loop can be unit-tested without sockets or
//! wall-clock pacing; [`HEADLESS_SERVER_BIN`](super::server) wires the real
//! process (signals, real-time slice, logging).
//!
//! ```
//! use racingedge::simulator::server::{HeadlessServer, ServerConfig};
//!
//! let mut server = HeadlessServer::new(ServerConfig {
//!     countdown: 0.0,
//!     time_limit: 100,
//!     duration: 30.0,
//!     ..ServerConfig::default()
//! });
//! server.start_offline();
//! let report = server.tick(2.0);
//! assert_eq!(report.phase, "Race");
//! assert!(!server.is_finished());
//! ```

use std::fmt;

use bevy::math::Vec3;

use crate::editor::ini::{parse_f32, parse_i32, Ini};
use crate::net::control_api::{ControlCommand, ControlHandler, ControlOut, ExternalControlApi};
use crate::net::discovery::ServerDiscovery;
use crate::simulator::feature_hub::{DEFAULT_CONTROL_PORT, DEFAULT_DISCOVERY_PORT};
use crate::simulator::multi_car::{AiController, MultiCarManager};
use crate::simulator::session_flow::SessionFlow;

/// Default session length when neither `--laps` nor `--time` is given.
pub const DEFAULT_TIME_LIMIT: i32 = 300;
/// Distance between grid slots spawned from a track spline.
pub const GRID_SPACING_M: f32 = 6.0;
/// Target speed held by the built-in pace driver (m/s).
const PACE_TARGET_SPEED: f32 = 55.0;

/// A scripted server configuration (INI keys plus command-line overrides).
#[derive(Clone, Debug, PartialEq)]
pub struct ServerConfig {
    /// Advertised host name (sanitized).
    pub name: String,
    /// Whether to announce the host on LAN discovery at startup.
    pub announce: bool,
    /// Track directory to load (empty = start without a track).
    pub track: String,
    /// Number of AI cars to spawn (0..=32).
    pub ai: i32,
    /// Advertised client capacity (1..=64).
    pub max_clients: i32,
    /// Discovery service UDP port.
    pub discovery_port: u16,
    /// Control API TCP port.
    pub control_port: u16,
    /// Race length in laps (needs `track_length`).
    pub laps: i32,
    /// Race time limit in seconds.
    pub time_limit: i32,
    /// Green-flag countdown in seconds.
    pub countdown: f32,
    /// Track length in metres (enables lap counting).
    pub track_length: f32,
    /// Hard sim-time cap in seconds.
    pub duration: f64,
    /// Status print interval in sim seconds (0 = off).
    pub status_every: f64,
    /// Skip real-time pacing.
    pub fast: bool,
    /// Record a binary replay to this path.
    pub replay: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            name: "racingedge-server".to_string(),
            announce: false,
            track: String::new(),
            ai: 0,
            max_clients: 24,
            discovery_port: DEFAULT_DISCOVERY_PORT,
            control_port: DEFAULT_CONTROL_PORT,
            laps: 0,
            time_limit: 0,
            countdown: 5.0,
            track_length: 0.0,
            duration: 600.0,
            status_every: 5.0,
            fast: false,
            replay: String::new(),
        }
    }
}

/// The outcome of [`parse_args`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// `--help` was requested; the string is the usage text.
    Help(String),
    /// The arguments or the config file are invalid.
    Invalid(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Help(text) => f.write_str(text),
            ConfigError::Invalid(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ConfigError {}

/// The command-line usage text.
#[must_use]
pub fn usage() -> String {
    format!(
        "racingedge-server - headless racing server\n\
         \x20 --config FILE        read defaults from FILE (default server/kssimserver.ini)\n\
         \x20 --announce           announce the host on LAN discovery\n\
         \x20 --track DIR          load a track folder\n\
         \x20 --ai N               number of AI cars (0-32)\n\
         \x20 --name NAME          advertised host name\n\
         \x20 --max-clients N      advertised capacity (1-64)\n\
         \x20 --discovery-port P   UDP discovery port (default {DEFAULT_DISCOVERY_PORT})\n\
         \x20 --control-port P     TCP control port (default {DEFAULT_CONTROL_PORT})\n\
         \x20 --laps N             race length in laps (needs --track-length)\n\
         \x20 --time S             race time limit in seconds\n\
         \x20 --countdown S        green-flag countdown (default 5)\n\
         \x20 --track-length M     track length in metres (enables lap counting)\n\
         \x20 --duration S         hard sim-time cap (default 600)\n\
         \x20 --status S           status print interval in sim seconds (0 = off)\n\
         \x20 --fast               no real-time pacing (as fast as possible)\n\
         \x20 --replay PATH        record a binary replay of the session\n"
    )
}

/// An index-based argument reader for the flag loop.
struct Args<'a> {
    items: &'a [String],
    index: usize,
}

impl<'a> Args<'a> {
    fn new(items: &'a [String]) -> Self {
        Self { items, index: 0 }
    }

    fn next(&mut self) -> Option<&'a str> {
        let value = self.items.get(self.index).map(String::as_str);
        self.index += 1;
        value
    }

    fn value(&mut self, flag: &str) -> Result<&'a str, ConfigError> {
        self.next()
            .ok_or_else(|| ConfigError::Invalid(format!("{flag} needs a value")))
    }
}

/// Parses the command line, applying `--config` defaults first so every other
/// flag overrides the file.
pub fn parse_args(args: &[String]) -> Result<ServerConfig, ConfigError> {
    let mut config = ServerConfig::default();

    let mut config_path = "server/kssimserver.ini".to_string();
    for (i, arg) in args.iter().enumerate() {
        if arg == "--config" {
            config_path = args
                .get(i + 1)
                .cloned()
                .ok_or_else(|| ConfigError::Invalid("--config needs a path".to_string()))?;
        }
    }
    if std::path::Path::new(&config_path).exists() {
        let ini = Ini::load(&config_path).map_err(ConfigError::Invalid)?;
        apply_ini(&mut config, &ini);
    }

    let mut args = Args::new(args);
    while let Some(arg) = args.next() {
        match arg {
            "--help" | "-h" => return Err(ConfigError::Help(usage())),
            "--config" => {
                args.next();
            }
            "--announce" => config.announce = true,
            "--fast" => config.fast = true,
            "--track" => {
                let value = args.value("--track")?;
                if !is_safe_path(value) {
                    return Err(ConfigError::Invalid(
                        "--track: rejected unsafe path".to_string(),
                    ));
                }
                config.track = value.to_string();
            }
            "--ai" => {
                config.ai = parse_i32(args.value("--ai")?)
                    .ok_or_else(|| ConfigError::Invalid("--ai: want a number".to_string()))?
                    .clamp(0, 32);
            }
            "--name" => config.name = sanitize_name(args.value("--name")?),
            "--max-clients" => {
                config.max_clients = parse_i32(args.value("--max-clients")?)
                    .ok_or_else(|| {
                        ConfigError::Invalid("--max-clients: want a number".to_string())
                    })?
                    .clamp(1, 64);
            }
            "--discovery-port" => {
                config.discovery_port =
                    parse_port(args.value("--discovery-port")?, "--discovery-port")?;
            }
            "--control-port" => {
                config.control_port = parse_port(args.value("--control-port")?, "--control-port")?;
            }
            "--laps" => {
                config.laps = parse_i32(args.value("--laps")?)
                    .ok_or_else(|| ConfigError::Invalid("--laps: want a number".to_string()))?;
            }
            "--time" => {
                config.time_limit = parse_i32(args.value("--time")?)
                    .ok_or_else(|| ConfigError::Invalid("--time: want a number".to_string()))?;
            }
            "--countdown" => {
                config.countdown = parse_f32(args.value("--countdown")?).ok_or_else(|| {
                    ConfigError::Invalid("--countdown: want a number".to_string())
                })?;
            }
            "--track-length" => {
                config.track_length =
                    parse_f32(args.value("--track-length")?).ok_or_else(|| {
                        ConfigError::Invalid("--track-length: want a number".to_string())
                    })?;
            }
            "--duration" => {
                config.duration = args
                    .value("--duration")?
                    .parse::<f64>()
                    .map_err(|_| ConfigError::Invalid("--duration: want a number".to_string()))?;
            }
            "--status" => {
                config.status_every = args
                    .value("--status")?
                    .parse::<f64>()
                    .map_err(|_| ConfigError::Invalid("--status: want a number".to_string()))?;
            }
            "--replay" => {
                let value = args.value("--replay")?;
                if !is_safe_path(value) {
                    return Err(ConfigError::Invalid(
                        "--replay: rejected unsafe path".to_string(),
                    ));
                }
                config.replay = value.to_string();
            }
            other => {
                return Err(ConfigError::Invalid(format!("unknown option '{other}'")));
            }
        }
    }

    config.name = sanitize_name(&config.name);
    if config.laps > 0 && config.track_length <= 0.0 {
        return Err(ConfigError::Invalid(
            "--laps requires --track-length".to_string(),
        ));
    }
    if config.laps <= 0 && config.time_limit <= 0 {
        config.time_limit = DEFAULT_TIME_LIMIT;
    }
    Ok(config)
}

/// Applies the `[server]` (or root) section of an INI file onto `config`.
fn apply_ini(config: &mut ServerConfig, ini: &Ini) {
    let Some(section) = ini.section("SERVER").or_else(|| ini.section("_ROOT")) else {
        return;
    };
    if let Some(value) = section.get("name") {
        config.name = sanitize_name(value);
    }
    if let Some(value) = section.get_bool("announce") {
        config.announce = value;
    }
    if let Some(value) = section.get("track") {
        if value.is_empty() || is_safe_path(value) {
            config.track = value.to_string();
        }
    }
    if let Some(value) = section.get("ai").and_then(parse_i32) {
        config.ai = value.clamp(0, 32);
    }
    if let Some(value) = section.get("max-clients").and_then(parse_i32) {
        config.max_clients = value.clamp(1, 64);
    }
    if let Some(value) = section.get("discovery-port").and_then(parse_u16) {
        config.discovery_port = value;
    }
    if let Some(value) = section.get("control-port").and_then(parse_u16) {
        config.control_port = value;
    }
    if let Some(value) = section.get("laps").and_then(parse_i32) {
        config.laps = value;
    }
    if let Some(value) = section.get("time").and_then(parse_i32) {
        config.time_limit = value;
    }
    if let Some(value) = section.get("countdown").and_then(parse_f32) {
        config.countdown = value;
    }
    if let Some(value) = section.get("track-length").and_then(parse_f32) {
        config.track_length = value;
    }
    if let Some(value) = section.get("duration") {
        if let Ok(parsed) = value.parse::<f64>() {
            config.duration = parsed;
        }
    }
    if let Some(value) = section.get("status") {
        if let Ok(parsed) = value.parse::<f64>() {
            config.status_every = parsed;
        }
    }
}

/// Keeps only `[A-Za-z0-9._ -]`, trims spaces and caps the length at 64.
#[must_use]
pub fn sanitize_name(input: &str) -> String {
    let mut out = String::with_capacity(input.len().min(64));
    for byte in input.bytes() {
        if out.len() >= 64 {
            break;
        }
        let c = byte as char;
        if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | ' ' | '.') {
            out.push(c);
        }
    }
    let trimmed = out.trim_matches(' ');
    if trimmed.is_empty() {
        "racingedge-server".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Whether `port` is a usable TCP/UDP port (1..=65535).
#[must_use]
pub fn is_valid_port(port: i32) -> bool {
    (1..=65535).contains(&port)
}

/// Rejects empty, oversized, NUL-containing or `..`-traversing paths.
#[must_use]
pub fn is_safe_path(path: &str) -> bool {
    if path.is_empty() || path.len() > 4096 || path.contains('\0') {
        return false;
    }
    let normalized = path.replace('\\', "/");
    !normalized.contains("/../") && !normalized.starts_with("../") && normalized != ".."
}

fn parse_port(value: &str, flag: &str) -> Result<u16, ConfigError> {
    let port = parse_u16(value)
        .ok_or_else(|| ConfigError::Invalid(format!("{flag}: want a port number (1-65535)")))?;
    if !is_valid_port(i32::from(port)) {
        return Err(ConfigError::Invalid(format!("{flag}: invalid port {port}")));
    }
    Ok(port)
}

fn parse_u16(value: &str) -> Option<u16> {
    value.trim().parse::<u16>().ok().filter(|port| *port != 0)
}

/// A snapshot of the running session.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerReport {
    /// The session phase name (`Idle`, `Countdown`, `Race`, `Finished`).
    pub phase: String,
    /// Completed laps.
    pub lap: i32,
    /// Green-flag elapsed time in seconds.
    pub session_time: f64,
    /// Player-car speed in m/s.
    pub speed: f32,
    /// Whether the session has ended.
    pub finished: bool,
}

/// Applies control commands to the server and answers the client.
#[derive(Debug, Default)]
pub struct ServerControlHandler {
    /// Commands received since the last drain (handled by the server loop).
    pub commands: Vec<ControlCommand>,
    /// The latest status line, echoed back for `STATUS`.
    pub status: String,
}

impl ControlHandler for ServerControlHandler {
    fn handle_control(&mut self, cmd: &ControlCommand, out: &mut ControlOut) {
        if cmd.verb == "STATUS" {
            let status = if self.status.is_empty() {
                "running"
            } else {
                self.status.as_str()
            };
            out.reply(format!("OK {status}"));
            return;
        }
        out.reply(format!("OK {}", cmd.verb));
        self.commands.push(cmd.clone());
    }
}

/// The headless server: session flow, cars, discovery and control API.
pub struct HeadlessServer {
    config: ServerConfig,
    cars: MultiCarManager,
    flow: SessionFlow,
    discovery: ServerDiscovery,
    control: ExternalControlApi,
    handler: ServerControlHandler,
    player_id: i32,
    clock: f64,
    session_time: f64,
    distance: f64,
    laps: i32,
    services: bool,
    discovery_started: bool,
    control_started: bool,
    finished: bool,
    commands_handled: usize,
}

impl HeadlessServer {
    /// Builds a server for `config` (no sockets bound yet).
    #[must_use]
    pub fn new(config: ServerConfig) -> Self {
        let mut cars = MultiCarManager::new();
        let player_id = cars.add_car("player", "Player", Vec3::ZERO, true);
        for i in 0..config.ai.max(0) {
            let slot = i + 1;
            let start = Vec3::new(0.0, 0.0, -GRID_SPACING_M * slot as f32);
            let id = cars.add_car(&format!("ai{slot}"), &format!("AI {slot}"), start, false);
            if let Some(car) = cars.get_car_mut(id) {
                car.ai = Some(AiController::new());
            }
        }
        let mut discovery = ServerDiscovery::new();
        discovery.set_port(config.discovery_port);
        discovery.set_host_info(
            &config.name,
            if config.track.is_empty() {
                "unknown"
            } else {
                &config.track
            },
            config.control_port,
            session_type_code(&config),
            1,
            config.max_clients,
        );
        Self {
            config,
            cars,
            flow: SessionFlow::new(),
            discovery,
            control: ExternalControlApi::new(),
            handler: ServerControlHandler::default(),
            player_id,
            clock: 0.0,
            session_time: 0.0,
            distance: 0.0,
            laps: 0,
            services: false,
            discovery_started: false,
            control_started: false,
            finished: false,
            commands_handled: 0,
        }
    }

    /// Begins the session without binding any socket (used by tests and for a
    /// server run purely against the control API of another process).
    pub fn start_offline(&mut self) {
        self.flow
            .start_race_direct(self.config.laps.max(1), self.config.countdown.max(0.0));
    }

    /// Starts the session and binds LAN discovery and the control API.
    /// Returns whether both sockets came up.
    pub fn start(&mut self) -> bool {
        self.start_offline();
        self.discovery_started = self.discovery.start(self.config.announce).is_ok();
        self.control_started = self.control.start(self.config.control_port).is_ok();
        self.services = self.discovery_started || self.control_started;
        self.discovery_started && self.control_started
    }

    /// Stops the network services.
    pub fn stop(&mut self) {
        self.discovery.stop();
        self.control.stop();
        self.discovery_started = false;
        self.control_started = false;
        self.services = false;
    }

    /// Advances the server by `dt` seconds and returns a status snapshot.
    pub fn tick(&mut self, dt: f64) -> ServerReport {
        if self.finished {
            return self.report();
        }
        self.clock += dt;
        self.flow.tick(dt as f32);

        let green = self.flow.is_green();
        if green {
            self.drive_pace();
        }
        self.cars.update(dt as f32);

        if green {
            self.session_time += dt;
            let speed = self.player_speed();
            self.distance += f64::from(speed) * dt;
            if self.config.track_length > 0.0 {
                self.laps = (self.distance / f64::from(self.config.track_length)) as i32;
            }
            let out_of_time = self.config.time_limit > 0
                && self.session_time >= f64::from(self.config.time_limit);
            let out_of_laps = self.config.laps > 0 && self.laps >= self.config.laps;
            if out_of_time || out_of_laps {
                self.flow.finish_race(Vec::new());
                self.finished = true;
            }
        }
        if self.clock >= self.config.duration {
            self.flow.finish_race(Vec::new());
            self.finished = true;
        }

        if self.services {
            self.discovery.tick(self.clock);
            self.control.accept_if_needed();
            self.handler.status = self.status_line();
            self.control.poll(&mut self.handler);
            let commands = std::mem::take(&mut self.handler.commands);
            self.commands_handled += commands.len();
        }
        self.report()
    }

    /// The configured server settings.
    #[must_use]
    pub fn config(&self) -> &ServerConfig {
        &self.config
    }

    /// The current session phase name.
    #[must_use]
    pub fn phase(&self) -> &'static str {
        self.flow.phase().name()
    }

    /// Whether the session has ended.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Whether LAN discovery came up.
    #[must_use]
    pub fn discovery_started(&self) -> bool {
        self.discovery_started
    }

    /// Whether the control API came up.
    #[must_use]
    pub fn control_started(&self) -> bool {
        self.control_started
    }

    /// How many control commands have been handled.
    #[must_use]
    pub fn commands_handled(&self) -> usize {
        self.commands_handled
    }

    /// The number of cars in the field (player + AI).
    #[must_use]
    pub fn car_count(&self) -> usize {
        self.cars.car_count()
    }

    /// A one-line status summary.
    #[must_use]
    pub fn status_line(&self) -> String {
        format!(
            "t={:.1} phase={} lap={} speed={:.1}",
            self.clock,
            self.phase(),
            self.laps,
            self.player_speed()
        )
    }

    fn report(&self) -> ServerReport {
        ServerReport {
            phase: self.phase().to_string(),
            lap: self.laps,
            session_time: self.session_time,
            speed: self.player_speed(),
            finished: self.finished,
        }
    }

    fn player_speed(&self) -> f32 {
        self.cars
            .get_car(self.player_id)
            .map_or(0.0, |car| car.pose.speed)
    }

    /// Built-in pace driver: hold a target speed on the straight.
    fn drive_pace(&mut self) {
        if let Some(car) = self.cars.get_car_mut(self.player_id) {
            let speed = car.pose.speed;
            car.throttle = 0.0;
            car.brake = 0.0;
            if speed < PACE_TARGET_SPEED {
                car.throttle = 1.0;
            } else if speed > PACE_TARGET_SPEED + 4.0 {
                car.brake = 0.25;
            } else {
                car.throttle = 0.35;
            }
        }
    }
}

fn session_type_code(config: &ServerConfig) -> u8 {
    if config.laps > 0 {
        1 // race
    } else {
        2 // practice/time-limited
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_shared_ports() {
        let config = ServerConfig::default();
        assert_eq!(config.discovery_port, DEFAULT_DISCOVERY_PORT);
        assert_eq!(config.control_port, DEFAULT_CONTROL_PORT);
        assert_eq!(config.name, "racingedge-server");
        assert_eq!(config.duration, 600.0);
    }

    #[test]
    fn sanitize_name_keeps_safe_chars() {
        assert_eq!(sanitize_name("  My Server!  "), "My Server");
        assert_eq!(sanitize_name("???"), "racingedge-server");
        assert_eq!(sanitize_name("a".repeat(80).as_str()).len(), 64);
    }

    #[test]
    fn safe_path_rejects_traversal() {
        assert!(is_safe_path("tracks/monza"));
        assert!(is_safe_path("C:/tracks/monza"));
        assert!(!is_safe_path(""));
        assert!(!is_safe_path("../secret"));
        assert!(!is_safe_path("tracks/../../etc"));
        assert!(!is_safe_path("a\0b"));
    }

    #[test]
    fn port_validation() {
        assert!(is_valid_port(1));
        assert!(is_valid_port(20779));
        assert!(is_valid_port(65535));
        assert!(!is_valid_port(0));
        assert!(!is_valid_port(70000));
    }

    #[test]
    fn parse_defaults_to_time_limited_session() {
        let config = parse_args(&[]).expect("parses");
        assert_eq!(config.time_limit, DEFAULT_TIME_LIMIT);
        assert_eq!(config.laps, 0);
    }

    #[test]
    fn parse_flags_override() {
        let args: Vec<String> = [
            "--name",
            "Silverstone",
            "--ai",
            "3",
            "--time",
            "120",
            "--fast",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let config = parse_args(&args).expect("parses");
        assert_eq!(config.name, "Silverstone");
        assert_eq!(config.ai, 3);
        assert_eq!(config.time_limit, 120);
        assert!(config.fast);
    }

    #[test]
    fn laps_require_track_length() {
        let args: Vec<String> = ["--laps", "5"].iter().map(|s| s.to_string()).collect();
        assert!(matches!(parse_args(&args), Err(ConfigError::Invalid(_))));

        let args: Vec<String> = ["--laps", "5", "--track-length", "5000"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let config = parse_args(&args).expect("parses");
        assert_eq!(config.laps, 5);
        assert_eq!(config.track_length, 5000.0);
    }

    #[test]
    fn help_and_unknown_options() {
        let args: Vec<String> = ["--help"].iter().map(|s| s.to_string()).collect();
        assert!(matches!(parse_args(&args), Err(ConfigError::Help(_))));

        let args: Vec<String> = ["--nope"].iter().map(|s| s.to_string()).collect();
        assert!(matches!(parse_args(&args), Err(ConfigError::Invalid(_))));
    }

    #[test]
    fn config_file_supplies_defaults() {
        let dir =
            std::env::temp_dir().join(format!("racingedge-server-ini-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("kssimserver.ini");
        std::fs::write(
            &path,
            "[server]\nname = From File\nai = 7\ntime = 90\nannounce = true\n",
        )
        .unwrap();

        let args: Vec<String> = ["--config", path.to_str().unwrap(), "--time", "45"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let config = parse_args(&args).expect("parses");
        assert_eq!(config.name, "From File");
        assert_eq!(config.ai, 7);
        assert_eq!(config.time_limit, 45, "command line wins over the file");
        assert!(config.announce);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn server_reaches_green_and_builds_the_field() {
        let mut server = HeadlessServer::new(ServerConfig {
            ai: 3,
            countdown: 0.0,
            time_limit: 100,
            duration: 600.0,
            ..ServerConfig::default()
        });
        server.start_offline();
        assert_eq!(server.car_count(), 4);
        let report = server.tick(2.0);
        assert_eq!(report.phase, "Race");
        assert!(!report.finished);
        assert!(server.status_line().contains("phase=Race"));
    }

    #[test]
    fn server_counts_laps_and_finishes() {
        let mut server = HeadlessServer::new(ServerConfig {
            laps: 1,
            track_length: 60.0,
            countdown: 0.0,
            duration: 600.0,
            ..ServerConfig::default()
        });
        server.start_offline();
        for _ in 0..400 {
            if server.tick(0.1).finished {
                break;
            }
        }
        assert!(server.is_finished());
        assert_eq!(server.phase(), "Finished");
    }

    #[test]
    fn server_stops_at_duration() {
        let mut server = HeadlessServer::new(ServerConfig {
            countdown: 0.0,
            time_limit: 1000,
            duration: 1.0,
            ..ServerConfig::default()
        });
        server.start_offline();
        assert!(!server.tick(0.5).finished);
        assert!(server.tick(0.6).finished);
    }

    #[test]
    fn control_handler_replies_and_records() {
        use crate::net::control_api::parse_control_line;
        let mut handler = ServerControlHandler::default();
        let mut out = ControlOut::default();

        handler.handle_control(&parse_control_line("WEATHER 2"), &mut out);
        assert_eq!(handler.commands.len(), 1);
        assert_eq!(out.take(), vec!["OK WEATHER\n".to_string()]);

        handler.status = "t=1.0 phase=Race".to_string();
        let mut out = ControlOut::default();
        handler.handle_control(&parse_control_line("STATUS"), &mut out);
        assert_eq!(out.take(), vec!["OK t=1.0 phase=Race\n".to_string()]);
        assert_eq!(handler.commands.len(), 1, "STATUS is not recorded");
    }
}
