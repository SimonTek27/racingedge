//! Session modes, flags and race-session state — port of ksengine
//! `src/simulator/SessionController.h`, `RaceFlag.h` and `RaceSession.h`
//! (GPL-3.0). Original: <https://github.com/SimonTek27/ksengine>
//!
//! Three small pieces the runtime and the network layer share:
//!
//! * [`GameSessionMode`] / [`SessionStartParams`] — what the menu selects and
//!   the defaults each mode starts with (`SessionController.h`).
//! * [`RaceFlag`] — the flag enum shared by the session types (`RaceFlag.h`).
//! * [`RaceSession`] — the flag/penalty/phase state machine driven by the lap
//!   count and a countdown (`RaceSession.h`).

use std::fmt;

/// What the player selected in the menu (port of `GameSessionMode`).
///
/// The default is [`GameSessionMode::Race`], the mode `FeatureHub` starts in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GameSessionMode {
    /// Free practice.
    Practice = 0,
    /// Timed qualifying.
    Qualifying = 1,
    /// A race.
    #[default]
    Race = 2,
    /// Time attack (single hot lap against the clock).
    TimeAttack = 3,
    /// Hotlap.
    Hotlap = 4,
    /// Replay playback.
    Replay = 5,
}

impl GameSessionMode {
    /// The wire/UI name (port of `sessionModeName`).
    pub fn name(self) -> &'static str {
        match self {
            GameSessionMode::Practice => "PRACTICE",
            GameSessionMode::Qualifying => "QUALIFYING",
            GameSessionMode::Race => "RACE",
            GameSessionMode::TimeAttack => "TIME_ATTACK",
            GameSessionMode::Hotlap => "HOTLAP",
            GameSessionMode::Replay => "REPLAY",
        }
    }

    /// The net session type byte (port of `toNetSessionType`).
    pub fn to_net_session_type(self) -> u8 {
        self as u8
    }

    /// Maps a menu entry to a mode (port of `modeFromMenuEntry`).
    ///
    /// Matching is case-insensitive on the substrings the reference looks for.
    pub fn from_menu_entry(entry: &str) -> Self {
        let upper = entry.to_ascii_uppercase();
        if upper.contains("QUAL") {
            GameSessionMode::Qualifying
        } else if upper.contains("PRACT") {
            GameSessionMode::Practice
        } else if upper.contains("TIME") {
            GameSessionMode::TimeAttack
        } else if upper.contains("HOT") {
            GameSessionMode::Hotlap
        } else if upper.contains("REPLAY") {
            GameSessionMode::Replay
        } else {
            GameSessionMode::Race
        }
    }
}

/// Session start parameters (port of `SessionStartParams`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionStartParams {
    /// Race distance in laps (`0` for timed sessions).
    pub total_laps: i32,
    /// Qualifying duration in minutes.
    pub qualify_minutes: i32,
    /// Practice duration in minutes.
    pub practice_minutes: i32,
    /// Number of grid slots.
    pub grid_size: i32,
    /// Whether the race starts from a standing (rather than rolling) start.
    pub standing_start: bool,
    /// Whether track limits are enforced.
    pub track_limits: bool,
}

impl Default for SessionStartParams {
    fn default() -> Self {
        Self {
            total_laps: 10,
            qualify_minutes: 15,
            practice_minutes: 30,
            grid_size: 16,
            standing_start: true,
            track_limits: true,
        }
    }
}

/// The defaults a mode starts from (port of `defaultsForMode`).
pub fn defaults_for_mode(mode: GameSessionMode) -> SessionStartParams {
    let mut params = SessionStartParams::default();
    match mode {
        GameSessionMode::Practice => {
            params.total_laps = 0;
            params.practice_minutes = 30;
        }
        GameSessionMode::Qualifying => {
            params.total_laps = 0;
            params.qualify_minutes = 15;
        }
        GameSessionMode::TimeAttack | GameSessionMode::Hotlap => {
            params.total_laps = 0;
        }
        GameSessionMode::Race => {
            params.total_laps = 10;
        }
        GameSessionMode::Replay => {}
    }
    params
}

/// Race flag (port of `RaceFlag`).
///
/// Meanings follow the standard motorsport flag rules. The numeric codes keep
/// the values the network/telemetry bridge already relies on; the flags added
/// later are appended.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RaceFlag {
    /// No flag shown.
    #[default]
    None = 0,
    /// The danger is over and the race resumes as normal.
    Green = 1,
    /// Hazard nearby: slow down and no overtaking.
    Yellow = 2,
    /// A car is about to be lapped (or already lapped) or must give way; three
    /// ignored blue flags earn a penalty.
    Blue = 3,
    /// Final lap (or a slow car on track).
    White = 4,
    /// Disqualification; the driver's number is displayed.
    Black = 5,
    /// End of the race (black-and-white chequered flag).
    Checkered = 6,
    /// Black with an orange circle: a car with a technical problem that may
    /// endanger other drivers.
    Meatball = 7,
    /// Safety car deployed.
    SafetyCar = 8,
    /// Hazard right where the flags are shown: slow down, no overtaking, and be
    /// prepared to deviate from the racing line.
    DoubleYellow = 9,
    /// Yellow with red stripes: slippery track surface, slow down.
    YellowRedStripes = 10,
    /// White and black: warning that the driver will be penalised for the next
    /// offence.
    WhiteAndBlack = 11,
    /// The session is suspended; during a race drivers must return to the pits.
    Red = 12,
}

/// Race session type (port of `SessionType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SessionType {
    /// Practice.
    Practice = 0,
    /// Qualify.
    Qualify = 1,
    /// Race.
    #[default]
    Race = 2,
    /// Hotlap.
    Hotlap = 3,
}

/// Race session phase (port of `SessionPhase`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SessionPhase {
    /// Not started.
    #[default]
    Idle = 0,
    /// Countdown to the start.
    Countdown = 1,
    /// Green flag.
    Green = 2,
    /// Yellow flag.
    Yellow = 3,
    /// Safety car.
    SafetyCar = 4,
    /// Checkered flag.
    Checkered = 5,
    /// Session over.
    Finished = 6,
    /// Session suspended (red flag).
    Red = 7,
}

/// Penalty kind (port of `PenaltyType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum PenaltyType {
    /// No penalty.
    #[default]
    None = 0,
    /// Drive-through.
    DriveThrough = 1,
    /// Stop-and-go.
    StopGo = 2,
    /// Time added.
    TimeSeconds = 3,
    /// Grid penalty.
    GridDrop = 4,
    /// Disqualified.
    Disqualified = 5,
}

/// A session penalty (port of `SessionPenalty`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SessionPenalty {
    /// Penalty kind.
    pub kind: PenaltyType,
    /// Time value for [`PenaltyType::TimeSeconds`].
    pub time_seconds: f32,
    /// Why the penalty was given.
    pub reason: String,
    /// Whether it has been served.
    pub served: bool,
}

/// Full race-session state (port of `RaceSessionState`).
#[derive(Clone, Debug, PartialEq)]
pub struct RaceSessionState {
    /// Session type.
    pub session_type: SessionType,
    /// Current phase.
    pub phase: SessionPhase,
    /// Current flag.
    pub flag: RaceFlag,
    /// Lap the leader is on.
    pub current_lap: i32,
    /// Total laps (`0` for timed sessions).
    pub total_laps: i32,
    /// Session clock or countdown, in seconds.
    pub time_remaining: f64,
    /// Current position of the player.
    pub position: i32,
    /// Number of cars in the session.
    pub total_cars: i32,
    /// Whether the pit lane is open.
    pub pit_lane_open: bool,
    /// Whether the mandatory pit stop has been served.
    pub mandatory_pit_done: bool,
    /// Outstanding and served penalties.
    pub penalties: Vec<SessionPenalty>,
}

impl Default for RaceSessionState {
    fn default() -> Self {
        Self {
            session_type: SessionType::Race,
            phase: SessionPhase::Idle,
            flag: RaceFlag::None,
            current_lap: 0,
            total_laps: 0,
            time_remaining: 0.0,
            position: 1,
            total_cars: 1,
            pit_lane_open: true,
            mandatory_pit_done: false,
            penalties: Vec::new(),
        }
    }
}

/// Flag/penalty/phase state machine (port of `RaceSession`).
#[derive(Clone, Debug, Default)]
pub struct RaceSession {
    state: RaceSessionState,
}

impl RaceSession {
    /// A fresh session in [`SessionPhase::Idle`].
    pub fn new() -> Self {
        Self::default()
    }

    /// The current state.
    pub fn state(&self) -> &RaceSessionState {
        &self.state
    }

    /// A mutable handle on the state.
    pub fn state_mut(&mut self) -> &mut RaceSessionState {
        &mut self.state
    }

    /// Resets to a countdown for `total_laps` (port of `configure`).
    pub fn configure(&mut self, session_type: SessionType, total_laps: i32, countdown_sec: f64) {
        self.state = RaceSessionState {
            session_type,
            total_laps,
            time_remaining: countdown_sec,
            phase: SessionPhase::Countdown,
            flag: RaceFlag::None,
            ..Default::default()
        };
    }

    /// Drops the green flag and starts the clock.
    pub fn start_green(&mut self) {
        self.state.phase = SessionPhase::Green;
        self.state.flag = RaceFlag::Green;
        self.state.time_remaining = 0.0;
    }

    /// Sets the flag and the phase it implies (port of `setFlag`).
    pub fn set_flag(&mut self, flag: RaceFlag) {
        self.state.flag = flag;
        match flag {
            RaceFlag::Yellow | RaceFlag::DoubleYellow | RaceFlag::YellowRedStripes => {
                self.state.phase = SessionPhase::Yellow;
            }
            RaceFlag::Red => self.state.phase = SessionPhase::Red,
            RaceFlag::SafetyCar => self.state.phase = SessionPhase::SafetyCar,
            RaceFlag::Checkered => self.state.phase = SessionPhase::Checkered,
            RaceFlag::Green if self.state.phase != SessionPhase::Finished => {
                self.state.phase = SessionPhase::Green;
            }
            _ => {}
        }
    }

    /// Adds a penalty to the session.
    pub fn add_penalty(&mut self, kind: PenaltyType, seconds: f32, reason: &str) {
        self.state.penalties.push(SessionPenalty {
            kind,
            time_seconds: seconds,
            reason: reason.to_string(),
            served: false,
        });
    }

    /// Advances the countdown / checkered logic. Returns `true` when the phase
    /// changed (port of `update`).
    pub fn update(&mut self, dt: f64, completed_laps: i32) -> bool {
        let previous = self.state.phase;
        match self.state.phase {
            SessionPhase::Countdown => {
                self.state.time_remaining -= dt;
                if self.state.time_remaining <= 0.0 {
                    self.start_green();
                }
            }
            SessionPhase::Green | SessionPhase::Yellow => {
                self.state.current_lap = completed_laps;
                if self.state.total_laps > 0 && completed_laps >= self.state.total_laps {
                    self.state.phase = SessionPhase::Checkered;
                    self.state.flag = RaceFlag::Checkered;
                }
            }
            SessionPhase::Checkered => {
                self.state.phase = SessionPhase::Finished;
            }
            _ => {}
        }
        self.state.phase != previous
    }

    /// Whether driving is allowed in the current phase.
    pub fn is_driving_allowed(&self) -> bool {
        matches!(
            self.state.phase,
            SessionPhase::Green | SessionPhase::Yellow | SessionPhase::SafetyCar
        )
    }

    /// Number of penalties still to serve.
    pub fn pending_penalties(&self) -> i32 {
        self.state.penalties.iter().filter(|p| !p.served).count() as i32
    }
}

impl fmt::Display for GameSessionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_names_match_reference() {
        assert_eq!(GameSessionMode::Practice.name(), "PRACTICE");
        assert_eq!(GameSessionMode::Qualifying.name(), "QUALIFYING");
        assert_eq!(GameSessionMode::Race.name(), "RACE");
        assert_eq!(GameSessionMode::TimeAttack.name(), "TIME_ATTACK");
        assert_eq!(GameSessionMode::Hotlap.name(), "HOTLAP");
        assert_eq!(GameSessionMode::Replay.name(), "REPLAY");
        assert_eq!(GameSessionMode::Race.to_net_session_type(), 2);
    }

    #[test]
    fn menu_entry_detection() {
        assert_eq!(
            GameSessionMode::from_menu_entry("Qualifying Practice"),
            GameSessionMode::Qualifying
        );
        assert_eq!(
            GameSessionMode::from_menu_entry("practice"),
            GameSessionMode::Practice
        );
        assert_eq!(
            GameSessionMode::from_menu_entry("TIME ATTACK"),
            GameSessionMode::TimeAttack
        );
        assert_eq!(
            GameSessionMode::from_menu_entry("Hotlap"),
            GameSessionMode::Hotlap
        );
        assert_eq!(
            GameSessionMode::from_menu_entry("Replay"),
            GameSessionMode::Replay
        );
        assert_eq!(
            GameSessionMode::from_menu_entry("Grand Prix"),
            GameSessionMode::Race
        );
    }

    #[test]
    fn defaults_for_mode_follow_reference() {
        assert_eq!(defaults_for_mode(GameSessionMode::Race).total_laps, 10);
        assert_eq!(defaults_for_mode(GameSessionMode::Practice).total_laps, 0);
        assert_eq!(
            defaults_for_mode(GameSessionMode::Practice).practice_minutes,
            30
        );
        assert_eq!(
            defaults_for_mode(GameSessionMode::Qualifying).qualify_minutes,
            15
        );
        assert_eq!(defaults_for_mode(GameSessionMode::Hotlap).total_laps, 0);
        assert_eq!(SessionStartParams::default().grid_size, 16);
    }

    #[test]
    fn race_session_countdown_then_green() {
        let mut session = RaceSession::new();
        session.configure(SessionType::Race, 3, 5.0);
        assert_eq!(session.state().phase, SessionPhase::Countdown);
        assert!(
            !session.update(4.0, 0),
            "phase unchanged before the countdown ends"
        );
        assert!(session.update(2.0, 0), "countdown ends");
        assert_eq!(session.state().phase, SessionPhase::Green);
        assert_eq!(session.state().flag, RaceFlag::Green);
        assert!(session.is_driving_allowed());
    }

    #[test]
    fn race_session_checkered_then_finished() {
        let mut session = RaceSession::new();
        session.configure(SessionType::Race, 2, 0.0);
        session.start_green();
        assert!(session.update(0.016, 2), "lap count reaches the distance");
        assert_eq!(session.state().phase, SessionPhase::Checkered);
        assert_eq!(session.state().flag, RaceFlag::Checkered);
        assert!(session.update(0.016, 2), "checkered becomes finished");
        assert_eq!(session.state().phase, SessionPhase::Finished);
    }

    #[test]
    fn flags_drive_phase() {
        let mut session = RaceSession::new();
        session.set_flag(RaceFlag::Yellow);
        assert_eq!(session.state().phase, SessionPhase::Yellow);
        session.set_flag(RaceFlag::DoubleYellow);
        assert_eq!(session.state().phase, SessionPhase::Yellow);
        session.set_flag(RaceFlag::YellowRedStripes);
        assert_eq!(session.state().phase, SessionPhase::Yellow);
        session.set_flag(RaceFlag::SafetyCar);
        assert_eq!(session.state().phase, SessionPhase::SafetyCar);
        session.set_flag(RaceFlag::Green);
        assert_eq!(session.state().phase, SessionPhase::Green);
    }

    #[test]
    fn red_flag_suspends_then_green_resumes() {
        let mut session = RaceSession::new();
        session.start_green();
        assert!(session.is_driving_allowed());
        session.set_flag(RaceFlag::Red);
        assert_eq!(session.state().phase, SessionPhase::Red);
        assert_eq!(session.state().flag, RaceFlag::Red);
        assert!(
            !session.is_driving_allowed(),
            "return to the pits under red"
        );
        session.set_flag(RaceFlag::Green);
        assert_eq!(session.state().phase, SessionPhase::Green);
        assert!(session.is_driving_allowed());
    }

    #[test]
    fn warning_flags_do_not_change_phase() {
        let mut session = RaceSession::new();
        session.start_green();
        session.set_flag(RaceFlag::WhiteAndBlack);
        assert_eq!(session.state().flag, RaceFlag::WhiteAndBlack);
        assert_eq!(session.state().phase, SessionPhase::Green);
        assert!(session.is_driving_allowed());
        session.set_flag(RaceFlag::Blue);
        assert_eq!(session.state().phase, SessionPhase::Green);
    }

    #[test]
    fn penalties_are_counted_until_served() {
        let mut session = RaceSession::new();
        session.add_penalty(PenaltyType::DriveThrough, 0.0, "cut");
        session.add_penalty(PenaltyType::TimeSeconds, 5.0, "speeding");
        assert_eq!(session.pending_penalties(), 2);
        session.state_mut().penalties[0].served = true;
        assert_eq!(session.pending_penalties(), 1);
    }

    #[test]
    fn configure_resets_previous_state() {
        let mut session = RaceSession::new();
        session.add_penalty(PenaltyType::StopGo, 0.0, "x");
        session.configure(SessionType::Qualify, 0, 3.0);
        assert_eq!(session.state().session_type, SessionType::Qualify);
        assert!(session.state().penalties.is_empty());
        assert_eq!(session.state().phase, SessionPhase::Countdown);
    }
}
