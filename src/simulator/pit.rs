//! Box, corsia dei box e servizio: uscita dal garage, coda, collisioni e
//! riparazioni in piazzola.
//!
//! Port di `src/simulator/GarageExit.{h}`, `PitLaneQueue.h`,
//! `PitLaneCollision.h`, `PitLaneRepair.h` e `PitStrategyBridge.h` di ksengine.

use super::damage::{DamageSystem, DamageType};
use super::session::{SessionPhase, SessionType};

// ── Uscita dal box (GarageExit) ─────────────────────────────────────────────

/// Posa del box nel mondo (sottoinsieme di `WorldPose`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GarageBoxPose {
    /// Coordinata X.
    pub x: f32,
    /// Coordinata Z.
    pub z: f32,
    /// Direzione (rad).
    pub heading: f32,
}

/// Fase dell'uscita dal box (port di `GarageExitPhase`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GarageExitPhase {
    /// Parcheggiata in box.
    #[default]
    InGarage,
    /// Uscita richiesta.
    Preparing,
    /// Avviamento motore.
    EngineStart,
    /// Attesa box libero.
    BoxClear,
    /// Uscita dal box verso la corsia.
    RollingOut,
    /// In corsia box, limitatore attivo.
    PitLane,
    /// Immissione in pista.
    TrackEntry,
    /// In pista.
    OnTrack,
    /// Ritorno al box.
    Returning,
    /// Bloccata.
    Blocked,
}

/// Motivo del blocco (port di `GarageBlockReason`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GarageBlockReason {
    /// Nessuno.
    #[default]
    None,
    /// Sessione bloccata.
    SessionLocked,
    /// Corsia chiusa.
    PitClosed,
    /// Motore spento.
    EngineOff,
    /// Percorso occupato.
    PathOccupied,
    /// Cancello chiuso.
    DoorClosed,
    /// Velocità troppo alta.
    SpeedTooHigh,
    /// Annullato dall'utente.
    UserCancel,
}

/// Configurazione dell'uscita dal box (port di `GarageExitConfig`).
#[derive(Clone, Copy, Debug)]
pub struct GarageExitConfig {
    /// Attesa massima in Preparing/BoxClear (s).
    pub prepare_timeout_sec: f32,
    /// Tempo minimo in EngineStart (s).
    pub engine_start_sec: f32,
    /// Distanza dal box per considerarla uscita (m).
    pub roll_out_distance_m: f32,
    /// Velocità massima con limitatore (km/h).
    pub pit_limiter_kmh: f32,
    /// Lunghezza della corsia box (m).
    pub pit_lane_length_m: f32,
    /// Lunghezza della fase di immissione (m).
    pub track_entry_blend_m: f32,
    /// Semi-larghezza del cono di uscita (m).
    pub exit_cone_half_width_m: f32,
    /// Lunghezza del cono di uscita (m).
    pub exit_cone_length_m: f32,
    /// Richiede il motore acceso.
    pub require_engine_on: bool,
    /// Avvia il motore automaticamente alla richiesta.
    pub auto_start_engine_on_request: bool,
    /// Congela l'auto finché è in box.
    pub hold_car_while_in_garage: bool,
    /// Consente l'uscita in gara.
    pub allow_exit_in_race: bool,
}

impl Default for GarageExitConfig {
    fn default() -> Self {
        Self {
            prepare_timeout_sec: 8.0,
            engine_start_sec: 1.2,
            roll_out_distance_m: 12.0,
            pit_limiter_kmh: 60.0,
            pit_lane_length_m: 80.0,
            track_entry_blend_m: 25.0,
            exit_cone_half_width_m: 2.5,
            exit_cone_length_m: 15.0,
            require_engine_on: true,
            auto_start_engine_on_request: true,
            hold_car_while_in_garage: true,
            allow_exit_in_race: false,
        }
    }
}

/// Ingresso del tick (port di `GarageExitInput`).
#[derive(Clone, Copy, Debug)]
pub struct GarageExitInput {
    /// Uscita richiesta.
    pub request_leave: bool,
    /// Annullamento richiesto.
    pub request_cancel: bool,
    /// Ritorno al box richiesto.
    pub request_return: bool,
    /// Motore acceso.
    pub engine_running: bool,
    /// Quadro acceso.
    pub ignition_on: bool,
    /// Velocità (m/s).
    pub speed_ms: f32,
    /// Acceleratore.
    pub throttle: f32,
    /// Freno.
    pub brake: f32,
    /// Sterzo.
    pub steer: f32,
    /// Posizione X.
    pub pos_x: f32,
    /// Posizione Y.
    pub pos_y: f32,
    /// Posizione Z.
    pub pos_z: f32,
    /// Direzione (rad).
    pub heading: f32,
    /// Un altro veicolo interseca il cono di uscita.
    pub path_blocked: bool,
    /// Corsia box aperta.
    pub pit_lane_open: bool,
    /// Tipo di sessione.
    pub session: SessionType,
    /// Fase della sessione.
    pub session_phase: SessionPhase,
}

impl Default for GarageExitInput {
    fn default() -> Self {
        Self {
            request_leave: false,
            request_cancel: false,
            request_return: false,
            engine_running: false,
            ignition_on: false,
            speed_ms: 0.0,
            throttle: 0.0,
            brake: 0.0,
            steer: 0.0,
            pos_x: 0.0,
            pos_y: 0.0,
            pos_z: 0.0,
            heading: 0.0,
            path_blocked: false,
            pit_lane_open: true,
            session: SessionType::Practice,
            session_phase: SessionPhase::Green,
        }
    }
}

/// Uscita del tick (port di `GarageExitOutput`).
#[derive(Clone, Debug, Default)]
pub struct GarageExitOutput {
    /// Fase corrente.
    pub phase: GarageExitPhase,
    /// Motivo del blocco.
    pub block_reason: GarageBlockReason,
    /// Ignora l'acceleratore del pilota.
    pub hold_controls: bool,
    /// Limitatore corsia attivo.
    pub pit_limiter_active: bool,
    /// Velocità massima del limitatore (m/s).
    pub pit_limiter_max_ms: f32,
    /// Il motore deve girare.
    pub engine_should_run: bool,
    /// La fisica può integrare il moto.
    pub allow_drive: bool,
    /// Aggancia alla posa del box.
    pub snap_to_box: bool,
    /// Posa del box.
    pub box_pose: GarageBoxPose,
    /// Testo di stato per la UI.
    pub status_text: String,
}

/// Controller dell'uscita dal box (port di `GarageExitController`).
pub struct GarageExitController {
    cfg: GarageExitConfig,
    phase: GarageExitPhase,
    block: GarageBlockReason,
    garage_index: i32,
    box_pose: GarageBoxPose,
    pit_heading: f32,
    timer: f32,
    dist_along_exit: f32,
    last_x: f32,
    last_z: f32,
    on_phase_changed: Option<Box<dyn FnMut(GarageExitPhase, GarageExitPhase)>>,
}

impl Default for GarageExitController {
    fn default() -> Self {
        Self::new()
    }
}

impl GarageExitController {
    /// Un controller con configurazione predefinita.
    pub fn new() -> Self {
        Self {
            cfg: GarageExitConfig::default(),
            phase: GarageExitPhase::InGarage,
            block: GarageBlockReason::None,
            garage_index: 0,
            box_pose: GarageBoxPose::default(),
            pit_heading: 0.0,
            timer: 0.0,
            dist_along_exit: 0.0,
            last_x: 0.0,
            last_z: 0.0,
            on_phase_changed: None,
        }
    }

    /// Imposta la configurazione.
    pub fn set_config(&mut self, cfg: GarageExitConfig) {
        self.cfg = cfg;
    }

    /// La configurazione.
    pub fn config(&self) -> &GarageExitConfig {
        &self.cfg
    }

    /// Registra la callback di cambio fase.
    pub fn set_on_phase_changed(
        &mut self,
        callback: impl FnMut(GarageExitPhase, GarageExitPhase) + 'static,
    ) {
        self.on_phase_changed = Some(Box::new(callback));
    }

    /// Associa il box.
    pub fn bind_box(&mut self, garage_index: i32, box_pose: GarageBoxPose, pit_heading: f32) {
        self.garage_index = garage_index;
        self.box_pose = box_pose;
        self.pit_heading = if pit_heading != 0.0 {
            pit_heading
        } else {
            box_pose.heading
        };
        self.phase = GarageExitPhase::InGarage;
        self.block = GarageBlockReason::None;
        self.timer = 0.0;
        self.dist_along_exit = 0.0;
        self.last_x = box_pose.x;
        self.last_z = box_pose.z;
    }

    /// La fase corrente.
    pub fn phase(&self) -> GarageExitPhase {
        self.phase
    }

    /// Il motivo del blocco.
    pub fn block_reason(&self) -> GarageBlockReason {
        self.block
    }

    /// L'indice del box.
    pub fn garage_index(&self) -> i32 {
        self.garage_index
    }

    /// Forza lo stato "in pista".
    pub fn force_on_track(&mut self) {
        self.phase = GarageExitPhase::OnTrack;
        self.block = GarageBlockReason::None;
        self.timer = 0.0;
    }

    /// Forza lo stato "in box".
    pub fn force_enter_garage(&mut self) {
        self.phase = GarageExitPhase::InGarage;
        self.block = GarageBlockReason::None;
        self.timer = 0.0;
        self.dist_along_exit = 0.0;
    }

    /// Avanza la macchina a stati (port di `update`).
    pub fn update(&mut self, dt: f32, input: &GarageExitInput) -> GarageExitOutput {
        self.timer += dt;
        self.accumulate_distance(input);

        let mut out = GarageExitOutput {
            box_pose: self.box_pose,
            phase: self.phase,
            ..Default::default()
        };

        if input.request_cancel
            && self.phase != GarageExitPhase::OnTrack
            && self.phase != GarageExitPhase::TrackEntry
        {
            self.phase = GarageExitPhase::InGarage;
            self.block = GarageBlockReason::UserCancel;
            self.timer = 0.0;
            self.dist_along_exit = 0.0;
        }

        if input.request_return && self.phase == GarageExitPhase::OnTrack {
            self.phase = GarageExitPhase::Returning;
            self.timer = 0.0;
        }

        match self.phase {
            GarageExitPhase::InGarage => self.tick_in_garage(input, &mut out),
            GarageExitPhase::Preparing => self.tick_preparing(input, &mut out),
            GarageExitPhase::EngineStart => self.tick_engine_start(input, &mut out),
            GarageExitPhase::BoxClear => self.tick_box_clear(input, &mut out),
            GarageExitPhase::RollingOut => self.tick_rolling_out(input, &mut out),
            GarageExitPhase::PitLane => self.tick_pit_lane(input, &mut out),
            GarageExitPhase::TrackEntry => self.tick_track_entry(input, &mut out),
            GarageExitPhase::OnTrack => self.tick_on_track(input, &mut out),
            GarageExitPhase::Returning => self.tick_returning(input, &mut out),
            GarageExitPhase::Blocked => self.tick_blocked(input, &mut out),
        }

        out.phase = self.phase;
        out.block_reason = self.block;
        if out.status_text.is_empty() {
            out.status_text = Self::phase_label(self.phase, self.block).to_string();
        }
        out
    }

    /// Etichetta di stato leggibile (port di `phaseLabel`).
    pub fn phase_label(phase: GarageExitPhase, block: GarageBlockReason) -> &'static str {
        if phase == GarageExitPhase::Blocked {
            return match block {
                GarageBlockReason::SessionLocked => "SESSION LOCK",
                GarageBlockReason::PitClosed => "PIT CLOSED",
                GarageBlockReason::EngineOff => "ENGINE OFF",
                GarageBlockReason::PathOccupied => "PATH BLOCKED",
                GarageBlockReason::DoorClosed => "DOOR CLOSED",
                GarageBlockReason::UserCancel => "CANCELLED",
                GarageBlockReason::None | GarageBlockReason::SpeedTooHigh => "BLOCKED",
            };
        }
        match phase {
            GarageExitPhase::InGarage => "IN GARAGE",
            GarageExitPhase::Preparing => "PREPARING",
            GarageExitPhase::EngineStart => "ENGINE START",
            GarageExitPhase::BoxClear => "WAITING CLEAR",
            GarageExitPhase::RollingOut => "ROLLING OUT",
            GarageExitPhase::PitLane => "PIT LANE",
            GarageExitPhase::TrackEntry => "PIT EXIT",
            GarageExitPhase::OnTrack => "ON TRACK",
            GarageExitPhase::Returning => "RETURNING",
            GarageExitPhase::Blocked => "BLOCKED",
        }
    }

    /// Limita l'acceleratore per rispettare il limitatore corsia.
    pub fn apply_pit_limiter(speed_ms: f32, throttle: f32, max_ms: f32) -> f32 {
        if max_ms <= 0.0 {
            return throttle;
        }
        if speed_ms > max_ms * 0.98 {
            return throttle.min(0.05);
        }
        if speed_ms > max_ms * 0.85 {
            return throttle.min(0.35);
        }
        throttle
    }

    fn set_phase(&mut self, next: GarageExitPhase) {
        if next == self.phase {
            return;
        }
        let prev = self.phase;
        self.phase = next;
        self.timer = 0.0;
        self.block = GarageBlockReason::None;
        if let Some(callback) = self.on_phase_changed.as_mut() {
            callback(prev, next);
        }
    }

    fn accumulate_distance(&mut self, input: &GarageExitInput) {
        let dx = input.pos_x - self.last_x;
        let dz = input.pos_z - self.last_z;
        self.last_x = input.pos_x;
        self.last_z = input.pos_z;
        if matches!(
            self.phase,
            GarageExitPhase::RollingOut | GarageExitPhase::PitLane | GarageExitPhase::TrackEntry
        ) {
            let hx = self.pit_heading.sin();
            let hz = self.pit_heading.cos();
            let along = dx * hx + dz * hz;
            if along > 0.0 {
                self.dist_along_exit += along;
            }
        }
    }

    fn session_allows_leave(&self, input: &GarageExitInput) -> bool {
        match input.session {
            SessionType::Practice | SessionType::Qualify | SessionType::Hotlap => true,
            SessionType::Race => {
                if !self.cfg.allow_exit_in_race {
                    return false;
                }
                input.pit_lane_open && input.session_phase != SessionPhase::Countdown
            }
        }
    }

    fn dist_from_box(&self, input: &GarageExitInput) -> f32 {
        let dx = input.pos_x - self.box_pose.x;
        let dz = input.pos_z - self.box_pose.z;
        (dx * dx + dz * dz).sqrt()
    }

    fn tick_in_garage(&mut self, input: &GarageExitInput, out: &mut GarageExitOutput) {
        out.hold_controls = self.cfg.hold_car_while_in_garage;
        out.snap_to_box = self.cfg.hold_car_while_in_garage;
        out.allow_drive = !self.cfg.hold_car_while_in_garage;
        out.engine_should_run = input.engine_running;
        out.pit_limiter_active = false;

        if input.request_leave {
            if !self.session_allows_leave(input) {
                self.set_phase(GarageExitPhase::Blocked);
                self.block = if input.pit_lane_open {
                    GarageBlockReason::SessionLocked
                } else {
                    GarageBlockReason::PitClosed
                };
                return;
            }
            self.set_phase(GarageExitPhase::Preparing);
        }
    }

    fn tick_preparing(&mut self, input: &GarageExitInput, out: &mut GarageExitOutput) {
        out.hold_controls = true;
        out.snap_to_box = true;
        out.allow_drive = false;
        out.status_text = "PREPARING EXIT".to_string();

        if !self.session_allows_leave(input) {
            self.set_phase(GarageExitPhase::Blocked);
            self.block = GarageBlockReason::SessionLocked;
            return;
        }
        if !input.pit_lane_open {
            self.set_phase(GarageExitPhase::Blocked);
            self.block = GarageBlockReason::PitClosed;
            return;
        }

        if self.cfg.require_engine_on && !input.engine_running {
            if self.cfg.auto_start_engine_on_request {
                out.engine_should_run = true;
                self.set_phase(GarageExitPhase::EngineStart);
                return;
            }
            out.engine_should_run = false;
            out.status_text = "START ENGINE".to_string();
            if self.timer > self.cfg.prepare_timeout_sec {
                self.set_phase(GarageExitPhase::Blocked);
                self.block = GarageBlockReason::EngineOff;
            }
            return;
        }

        out.engine_should_run = true;
        self.set_phase(GarageExitPhase::BoxClear);
    }

    fn tick_engine_start(&mut self, input: &GarageExitInput, out: &mut GarageExitOutput) {
        out.hold_controls = true;
        out.snap_to_box = true;
        out.allow_drive = false;
        out.engine_should_run = true;
        out.status_text = "ENGINE START".to_string();

        if input.engine_running && self.timer >= self.cfg.engine_start_sec {
            self.set_phase(GarageExitPhase::BoxClear);
        } else if self.timer > self.cfg.prepare_timeout_sec + self.cfg.engine_start_sec {
            self.set_phase(GarageExitPhase::Blocked);
            self.block = GarageBlockReason::EngineOff;
        }
    }

    fn tick_box_clear(&mut self, input: &GarageExitInput, out: &mut GarageExitOutput) {
        out.hold_controls = true;
        out.snap_to_box = true;
        out.allow_drive = false;
        out.engine_should_run = true;

        if input.path_blocked {
            out.status_text = "WAITING — PATH BUSY".to_string();
            self.block = GarageBlockReason::PathOccupied;
            if self.timer > self.cfg.prepare_timeout_sec {
                self.set_phase(GarageExitPhase::Blocked);
                self.block = GarageBlockReason::PathOccupied;
            }
            return;
        }

        self.block = GarageBlockReason::None;
        out.status_text = "CLEAR — GO".to_string();
        if self.timer > 0.35 {
            self.dist_along_exit = 0.0;
            self.set_phase(GarageExitPhase::RollingOut);
        }
    }

    fn tick_rolling_out(&mut self, input: &GarageExitInput, out: &mut GarageExitOutput) {
        out.hold_controls = false;
        out.snap_to_box = false;
        out.allow_drive = true;
        out.engine_should_run = true;
        out.pit_limiter_active = true;
        out.pit_limiter_max_ms = self.cfg.pit_limiter_kmh / 3.6;
        out.status_text = "PIT LIMIT".to_string();

        if input.path_blocked && self.dist_from_box(input) < self.cfg.roll_out_distance_m * 0.5 {
            out.hold_controls = true;
            out.status_text = "YIELD".to_string();
            return;
        }

        if self.dist_from_box(input) >= self.cfg.roll_out_distance_m
            || self.dist_along_exit >= self.cfg.roll_out_distance_m
        {
            self.set_phase(GarageExitPhase::PitLane);
        }
    }

    fn tick_pit_lane(&mut self, input: &GarageExitInput, out: &mut GarageExitOutput) {
        out.hold_controls = false;
        out.allow_drive = true;
        out.engine_should_run = true;
        out.pit_limiter_active = true;
        out.pit_limiter_max_ms = self.cfg.pit_limiter_kmh / 3.6;
        out.status_text = "PIT LANE".to_string();

        if self.dist_along_exit >= self.cfg.roll_out_distance_m + self.cfg.pit_lane_length_m {
            self.set_phase(GarageExitPhase::TrackEntry);
        }
        if self.dist_from_box(input)
            > self.cfg.roll_out_distance_m + self.cfg.pit_lane_length_m * 0.6
            && input.speed_ms > 5.0
        {
            self.set_phase(GarageExitPhase::TrackEntry);
        }
    }

    fn tick_track_entry(&mut self, input: &GarageExitInput, out: &mut GarageExitOutput) {
        let _ = input;
        out.allow_drive = true;
        out.engine_should_run = true;
        let t = (self.timer / 2.5).clamp(0.0, 1.0);
        out.pit_limiter_active = t < 0.85;
        out.pit_limiter_max_ms = (self.cfg.pit_limiter_kmh / 3.6) * (1.0 + t * 2.0);
        out.status_text = "PIT EXIT".to_string();

        if self.timer > 2.5
            || self.dist_along_exit
                > self.cfg.roll_out_distance_m
                    + self.cfg.pit_lane_length_m
                    + self.cfg.track_entry_blend_m
        {
            self.set_phase(GarageExitPhase::OnTrack);
        }
    }

    fn tick_on_track(&mut self, _input: &GarageExitInput, out: &mut GarageExitOutput) {
        out.allow_drive = true;
        out.engine_should_run = true;
        out.pit_limiter_active = false;
        out.hold_controls = false;
    }

    fn tick_returning(&mut self, input: &GarageExitInput, out: &mut GarageExitOutput) {
        out.allow_drive = true;
        out.pit_limiter_active = true;
        out.pit_limiter_max_ms = self.cfg.pit_limiter_kmh / 3.6;
        out.engine_should_run = true;
        out.status_text = "TO GARAGE".to_string();

        if self.dist_from_box(input) < 3.0 && input.speed_ms < 1.5 {
            self.set_phase(GarageExitPhase::InGarage);
            self.dist_along_exit = 0.0;
        }
    }

    fn tick_blocked(&mut self, input: &GarageExitInput, out: &mut GarageExitOutput) {
        out.hold_controls = true;
        out.snap_to_box = self.block != GarageBlockReason::PathOccupied;
        out.allow_drive = false;
        out.engine_should_run = input.engine_running;

        if input.request_leave && self.session_allows_leave(input) && input.pit_lane_open {
            match self.block {
                GarageBlockReason::PathOccupied if !input.path_blocked => {
                    self.set_phase(GarageExitPhase::BoxClear);
                    return;
                }
                GarageBlockReason::EngineOff if input.engine_running => {
                    self.set_phase(GarageExitPhase::BoxClear);
                    return;
                }
                GarageBlockReason::PitClosed => {
                    self.set_phase(GarageExitPhase::Preparing);
                    return;
                }
                GarageBlockReason::UserCancel | GarageBlockReason::SessionLocked
                    if self.session_allows_leave(input) =>
                {
                    self.set_phase(GarageExitPhase::Preparing);
                }
                _ => {}
            }
        }
        if !input.request_leave && self.block == GarageBlockReason::UserCancel {
            self.set_phase(GarageExitPhase::InGarage);
        }
    }
}

/// Test di occupazione del cono di uscita sul piano XZ (port di
/// `isExitPathBlocked`).
pub fn is_exit_path_blocked(
    box_pose: &GarageBoxPose,
    pit_heading: f32,
    cone_len: f32,
    cone_half_width: f32,
    others: &[(f32, f32)],
    self_x: f32,
    self_z: f32,
) -> bool {
    let hx = pit_heading.sin();
    let hz = pit_heading.cos();
    let lx = (pit_heading + std::f32::consts::FRAC_PI_2).sin();
    let lz = (pit_heading + std::f32::consts::FRAC_PI_2).cos();

    for &(ox, oz) in others {
        let dx = ox - box_pose.x;
        let dz = oz - box_pose.z;
        let dsx = ox - self_x;
        let dsz = oz - self_z;
        if dsx * dsx + dsz * dsz < 0.25 {
            continue;
        }

        let along = dx * hx + dz * hz;
        let lat = dx * lx + dz * lz;
        if along > 0.5 && along < cone_len && lat.abs() < cone_half_width {
            return true;
        }
    }
    false
}

// ── Coda corsia box (PitLaneQueue) ──────────────────────────────────────────

/// Ruolo in coda (port di `PitQueueRole`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PitQueueRole {
    /// Dal box alla pista.
    #[default]
    Leaving,
    /// Dalla pista al box.
    Entering,
    /// Ferma in box/servizio.
    Holding,
}

/// Stato in coda (port di `PitQueueStatus`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PitQueueStatus {
    /// In attesa.
    #[default]
    Waiting,
    /// Autorizzata a muoversi.
    ClearedToMove,
    /// In movimento.
    Moving,
    /// Conclusa.
    Done,
}

/// Voce della coda (port di `PitQueueEntry`).
#[derive(Clone, Copy, Debug)]
pub struct PitQueueEntry {
    /// Id auto.
    pub car_id: i32,
    /// Numero di gara.
    pub race_number: i32,
    /// Indice box.
    pub garage_index: i32,
    /// Ruolo.
    pub role: PitQueueRole,
    /// Stato.
    pub status: PitQueueStatus,
    /// Istante di ingresso in coda.
    pub join_time: f32,
    /// Posizione sull'asse corsia (m).
    pub along_pit: f32,
    /// Velocità (m/s).
    pub speed_ms: f32,
    /// Posizione X.
    pub pos_x: f32,
    /// Posizione Z.
    pub pos_z: f32,
    /// Priorità (più alta = preferita).
    pub priority: i32,
    /// È il giocatore.
    pub is_player: bool,
}

impl Default for PitQueueEntry {
    fn default() -> Self {
        Self {
            car_id: -1,
            race_number: 0,
            garage_index: 0,
            role: PitQueueRole::Leaving,
            status: PitQueueStatus::Waiting,
            join_time: 0.0,
            along_pit: 0.0,
            speed_ms: 0.0,
            pos_x: 0.0,
            pos_z: 0.0,
            priority: 0,
            is_player: false,
        }
    }
}

/// Configurazione della coda (port di `PitLaneQueueConfig`).
#[derive(Clone, Copy, Debug)]
pub struct PitLaneQueueConfig {
    /// Spaziatura minima (m).
    pub min_spacing_m: f32,
    /// Distacco richiesto prima di liberare il successivo (m).
    pub release_gap_m: f32,
    /// Velocità massima in corsia (m/s).
    pub max_pit_speed_ms: f32,
    /// Origine dell'asse corsia (m).
    pub pit_start_along: f32,
    /// Fine dell'asse corsia (m).
    pub pit_end_along: f32,
    /// Offset del box sull'asse (m).
    pub box_pull_in_along: f32,
    /// Priorità al giocatore.
    pub player_priority: bool,
    /// Bonus di priorità del giocatore.
    pub player_priority_boost: i32,
    /// Attesa massima (s), per l'urgenza in UI.
    pub max_wait_sec: f32,
}

impl Default for PitLaneQueueConfig {
    fn default() -> Self {
        Self {
            min_spacing_m: 8.0,
            release_gap_m: 12.0,
            max_pit_speed_ms: 60.0 / 3.6,
            pit_start_along: 0.0,
            pit_end_along: 120.0,
            box_pull_in_along: 5.0,
            player_priority: true,
            player_priority_boost: 10,
            max_wait_sec: 45.0,
        }
    }
}

/// Asse 1D della corsia (port di `PitAxis`): `along = (p - origine)·direzione`.
#[derive(Clone, Copy, Debug, Default)]
pub struct PitAxis {
    /// Origine X.
    pub origin_x: f32,
    /// Origine Z.
    pub origin_z: f32,
    /// Direzione (rad).
    pub heading: f32,
}

impl PitAxis {
    /// Coordinata lungo l'asse.
    pub fn along(&self, x: f32, z: f32) -> f32 {
        let hx = self.heading.sin();
        let hz = self.heading.cos();
        (x - self.origin_x) * hx + (z - self.origin_z) * hz
    }

    /// Converte coordinata assiale + laterale in posizione nel mondo.
    pub fn world_from_along(&self, s: f32, lateral: f32) -> (f32, f32) {
        let hx = self.heading.sin();
        let hz = self.heading.cos();
        let lx = (self.heading + std::f32::consts::FRAC_PI_2).sin();
        let lz = (self.heading + std::f32::consts::FRAC_PI_2).cos();
        (
            self.origin_x + hx * s + lx * lateral,
            self.origin_z + hz * s + lz * lateral,
        )
    }
}

/// Coda della corsia box (port di `PitLaneQueue`).
pub struct PitLaneQueue {
    cfg: PitLaneQueueConfig,
    axis: PitAxis,
    entries: Vec<PitQueueEntry>,
    sim_time: f32,
}

impl Default for PitLaneQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl PitLaneQueue {
    /// Una coda vuota con configurazione predefinita.
    pub fn new() -> Self {
        Self {
            cfg: PitLaneQueueConfig::default(),
            axis: PitAxis::default(),
            entries: Vec::new(),
            sim_time: 0.0,
        }
    }

    /// Imposta la configurazione.
    pub fn set_config(&mut self, cfg: PitLaneQueueConfig) {
        self.cfg = cfg;
    }

    /// La configurazione.
    pub fn config(&self) -> &PitLaneQueueConfig {
        &self.cfg
    }

    /// Imposta l'asse della corsia.
    pub fn set_axis(&mut self, axis: PitAxis) {
        self.axis = axis;
    }

    /// L'asse della corsia.
    pub fn axis(&self) -> &PitAxis {
        &self.axis
    }

    /// Imposta il tempo di simulazione.
    pub fn set_sim_time(&mut self, time: f32) {
        self.sim_time = time;
    }

    /// Le voci in coda.
    pub fn entries(&self) -> &[PitQueueEntry] {
        &self.entries
    }

    /// Richiede di entrare nella coda di uscita (dal box).
    pub fn request_leave(
        &mut self,
        car_id: i32,
        race_number: i32,
        garage_index: i32,
        is_player: bool,
        pos_x: f32,
        pos_z: f32,
    ) {
        if self.find(car_id).is_some() {
            return;
        }
        let mut e = PitQueueEntry {
            car_id,
            race_number,
            garage_index,
            role: PitQueueRole::Leaving,
            status: PitQueueStatus::Waiting,
            join_time: self.sim_time,
            pos_x,
            pos_z,
            along_pit: self.axis.along(pos_x, pos_z),
            is_player,
            ..Default::default()
        };
        e.priority = if is_player && self.cfg.player_priority {
            self.cfg.player_priority_boost
        } else {
            0
        };
        e.priority += (50 - garage_index).max(0);
        self.entries.push(e);
        self.sort_queue();
    }

    /// Richiede di entrare nella coda di rientro (verso il box).
    pub fn request_enter(
        &mut self,
        car_id: i32,
        race_number: i32,
        garage_index: i32,
        is_player: bool,
        pos_x: f32,
        pos_z: f32,
    ) {
        if let Some(existing) = self.find_mut(car_id) {
            existing.role = PitQueueRole::Entering;
            existing.garage_index = garage_index;
            existing.status = PitQueueStatus::Waiting;
            return;
        }
        let mut e = PitQueueEntry {
            car_id,
            race_number,
            garage_index,
            role: PitQueueRole::Entering,
            status: PitQueueStatus::Waiting,
            join_time: self.sim_time,
            pos_x,
            pos_z,
            along_pit: self.axis.along(pos_x, pos_z),
            is_player,
            ..Default::default()
        };
        e.priority = if is_player && self.cfg.player_priority {
            self.cfg.player_priority_boost
        } else {
            0
        };
        self.entries.push(e);
        self.sort_queue();
    }

    /// Rimuove un'auto dalla coda.
    pub fn leave_queue(&mut self, car_id: i32) {
        self.entries.retain(|e| e.car_id != car_id);
    }

    /// Aggiorna la posizione di un'auto.
    pub fn update_car(&mut self, car_id: i32, pos_x: f32, pos_z: f32, speed_ms: f32) {
        let along = self.axis.along(pos_x, pos_z);
        if let Some(e) = self.find_mut(car_id) {
            e.pos_x = pos_x;
            e.pos_z = pos_z;
            e.speed_ms = speed_ms;
            e.along_pit = along;
        }
    }

    /// Ricalcola chi può muoversi in base alla spaziatura (port di `update`).
    pub fn update(&mut self, _dt: f32) {
        self.sort_queue();

        for e in &mut self.entries {
            if e.status == PitQueueStatus::Done || e.status == PitQueueStatus::Moving {
                continue;
            }
            e.status = PitQueueStatus::Waiting;
        }

        let mut last_released_along = -1e9_f32;
        for i in 0..self.entries.len() {
            if self.entries[i].role != PitQueueRole::Leaving
                || self.entries[i].status == PitQueueStatus::Done
            {
                continue;
            }
            if self.entries[i].status == PitQueueStatus::Moving {
                last_released_along = last_released_along.max(self.entries[i].along_pit);
                continue;
            }

            let gap_ok = last_released_along < -1e8
                || (last_released_along - self.entries[i].along_pit) >= self.cfg.release_gap_m;

            let mut conflict = false;
            for o in &self.entries {
                if o.car_id == self.entries[i].car_id || o.role == PitQueueRole::Holding {
                    continue;
                }
                let d = o.along_pit - self.entries[i].along_pit;
                if d > 0.0 && d < self.cfg.min_spacing_m {
                    conflict = true;
                }
            }

            if gap_ok && !conflict {
                self.entries[i].status = PitQueueStatus::ClearedToMove;
                last_released_along = self.entries[i].along_pit;
            } else {
                self.entries[i].status = PitQueueStatus::Waiting;
            }
        }

        for i in 0..self.entries.len() {
            if self.entries[i].role != PitQueueRole::Entering
                || self.entries[i].status == PitQueueStatus::Done
                || self.entries[i].status == PitQueueStatus::Moving
            {
                continue;
            }

            let mut blocked = false;
            for o in &self.entries {
                if o.car_id == self.entries[i].car_id {
                    continue;
                }
                let d = (o.along_pit - self.entries[i].along_pit).abs();
                if d < self.cfg.min_spacing_m && o.status != PitQueueStatus::Done {
                    blocked = true;
                }
            }
            self.entries[i].status = if blocked {
                PitQueueStatus::Waiting
            } else {
                PitQueueStatus::ClearedToMove
            };
        }
    }

    /// Segna un'auto come in movimento.
    pub fn mark_moving(&mut self, car_id: i32) {
        if let Some(e) = self.find_mut(car_id) {
            e.status = PitQueueStatus::Moving;
        }
    }

    /// Segna un'auto come conclusa e la rimuove.
    pub fn mark_done(&mut self, car_id: i32) {
        if let Some(e) = self.find_mut(car_id) {
            e.status = PitQueueStatus::Done;
        }
        self.leave_queue(car_id);
    }

    /// L'auto è autorizzata a muoversi.
    pub fn is_cleared(&self, car_id: i32) -> bool {
        self.find(car_id).is_some_and(|e| {
            e.status == PitQueueStatus::ClearedToMove || e.status == PitQueueStatus::Moving
        })
    }

    /// L'auto è in attesa.
    pub fn is_waiting(&self, car_id: i32) -> bool {
        self.find(car_id)
            .is_some_and(|e| e.status == PitQueueStatus::Waiting)
    }

    /// Posizione nella coda di uscita (0 = prossima), o -1.
    pub fn leave_queue_position(&self, car_id: i32) -> i32 {
        let mut pos = 0;
        for e in &self.entries {
            if e.role != PitQueueRole::Leaving || e.status == PitQueueStatus::Done {
                continue;
            }
            if e.car_id == car_id {
                return pos;
            }
            pos += 1;
        }
        -1
    }

    /// Lunghezza della coda di uscita.
    pub fn leave_queue_length(&self) -> i32 {
        self.entries
            .iter()
            .filter(|e| e.role == PitQueueRole::Leaving && e.status != PitQueueStatus::Done)
            .count() as i32
    }

    /// Se l'uscita dal box è da bloccare per quest'auto (port di
    /// `shouldBlockGarageExit`).
    pub fn should_block_garage_exit(&self, car_id: i32) -> bool {
        let Some(e) = self.find(car_id) else {
            return false;
        };
        if e.role != PitQueueRole::Leaving {
            return false;
        }
        if e.status == PitQueueStatus::ClearedToMove || e.status == PitQueueStatus::Moving {
            return false;
        }
        true
    }

    /// Velocità massima suggerita per un'auto in coda.
    pub fn suggested_max_speed_ms(&self, car_id: i32) -> f32 {
        let Some(e) = self.find(car_id) else {
            return self.cfg.max_pit_speed_ms;
        };
        if e.status == PitQueueStatus::Waiting {
            return 0.0;
        }
        let mut ahead = 1e9_f32;
        for o in &self.entries {
            if o.car_id == car_id || o.role == PitQueueRole::Holding {
                continue;
            }
            let d = o.along_pit - e.along_pit;
            if d > 0.5 && d < ahead {
                ahead = d;
            }
        }
        if ahead < self.cfg.min_spacing_m * 1.5 {
            return self.cfg.max_pit_speed_ms * 0.35;
        }
        self.cfg.max_pit_speed_ms
    }

    /// Stato leggibile per il debug.
    pub fn debug_status(&self, car_id: i32) -> String {
        let Some(e) = self.find(car_id) else {
            return "NOT IN QUEUE".to_string();
        };
        let st = match e.status {
            PitQueueStatus::Waiting => "WAIT",
            PitQueueStatus::ClearedToMove => "CLEAR",
            PitQueueStatus::Moving => "MOVE",
            PitQueueStatus::Done => "DONE",
        };
        let role = match e.role {
            PitQueueRole::Leaving => "LEAVE",
            PitQueueRole::Entering => "ENTER",
            PitQueueRole::Holding => "HOLD",
        };
        format!("{role} {st} q={}", self.leave_queue_position(car_id))
    }

    fn find(&self, car_id: i32) -> Option<&PitQueueEntry> {
        self.entries.iter().find(|e| e.car_id == car_id)
    }

    fn find_mut(&mut self, car_id: i32) -> Option<&mut PitQueueEntry> {
        self.entries.iter_mut().find(|e| e.car_id == car_id)
    }

    fn sort_queue(&mut self) {
        self.entries.sort_by(|a, b| {
            if a.role != b.role {
                return (a.role as i32).cmp(&(b.role as i32));
            }
            if a.role == PitQueueRole::Leaving {
                if a.priority != b.priority {
                    return b.priority.cmp(&a.priority);
                }
                return a
                    .join_time
                    .partial_cmp(&b.join_time)
                    .unwrap_or(std::cmp::Ordering::Equal);
            }
            a.along_pit
                .partial_cmp(&b.along_pit)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
}

/// Collega coda e uscita box per una singola auto (port di
/// `integratePitQueueWithGarageExit`).
pub fn integrate_pit_queue_with_garage_exit(
    queue: &mut PitLaneQueue,
    exit_ctrl: &GarageExitController,
    car_id: i32,
    input: &mut GarageExitInput,
    throttle: &mut f32,
) {
    if matches!(
        exit_ctrl.phase(),
        GarageExitPhase::Preparing | GarageExitPhase::BoxClear | GarageExitPhase::EngineStart
    ) && queue.should_block_garage_exit(car_id)
    {
        input.path_blocked = true;
    }

    if matches!(
        exit_ctrl.phase(),
        GarageExitPhase::RollingOut | GarageExitPhase::PitLane
    ) {
        queue.mark_moving(car_id);
        let vmax = queue.suggested_max_speed_ms(car_id);
        *throttle = GarageExitController::apply_pit_limiter(input.speed_ms, *throttle, vmax);
    }

    if exit_ctrl.phase() == GarageExitPhase::OnTrack {
        queue.mark_done(car_id);
    }
}

// ── Collisioni corsia box (PitLaneCollision) ────────────────────────────────

/// Corpo di un'auto in corsia (port di `PitCarBody`).
#[derive(Clone, Copy, Debug)]
pub struct PitCarBody {
    /// Id auto.
    pub car_id: i32,
    /// Posizione X.
    pub x: f32,
    /// Posizione Z.
    pub z: f32,
    /// Direzione (rad).
    pub heading: f32,
    /// Velocità X (m/s).
    pub vx: f32,
    /// Velocità Z (m/s).
    pub vz: f32,
    /// Lunghezza (m).
    pub length: f32,
    /// Larghezza (m).
    pub width: f32,
    /// Massa (kg).
    pub mass: f32,
    /// Attiva.
    pub active: bool,
    /// Invulnerabile (es. agganciata al box).
    pub invulnerable: bool,
}

impl Default for PitCarBody {
    fn default() -> Self {
        Self {
            car_id: -1,
            x: 0.0,
            z: 0.0,
            heading: 0.0,
            vx: 0.0,
            vz: 0.0,
            length: 4.6,
            width: 2.0,
            mass: 1200.0,
            active: true,
            invulnerable: false,
        }
    }
}

/// Contatto di collisione (port di `PitCollisionContact`).
#[derive(Clone, Copy, Debug, Default)]
pub struct PitCollisionContact {
    /// Auto A.
    pub car_a: i32,
    /// Car B, or -1 for the wall.
    pub car_b: i32,
    /// Normale X (da B verso A).
    pub nx: f32,
    /// Normale Z (da B verso A).
    pub nz: f32,
    /// Penetrazione.
    pub penetration: f32,
    /// Velocità di chiusura (m/s).
    pub rel_speed: f32,
    /// Impulso applicato.
    pub impulse: f32,
    /// Contatto con il muro.
    pub with_wall: bool,
}

/// Configurazione delle collisioni (port di `PitLaneCollisionConfig`).
#[derive(Clone, Copy, Debug)]
pub struct PitLaneCollisionConfig {
    /// Semi-larghezza del corridoio (m).
    pub pit_half_width_m: f32,
    /// Restituzione.
    pub restitution: f32,
    /// Attrito.
    pub friction: f32,
    /// Percentuale di separazione posizionale.
    pub separation_percent: f32,
    /// Impulso massimo (N·s).
    pub max_impulse: f32,
    /// Soglia di velocità per il danno (m/s).
    pub damage_speed_threshold: f32,
    /// Restituzione contro il muro.
    pub wall_restitution: f32,
    /// Abilita i muri.
    pub enable_walls: bool,
    /// Abilita auto-auto.
    pub enable_car_car: bool,
}

impl Default for PitLaneCollisionConfig {
    fn default() -> Self {
        Self {
            pit_half_width_m: 3.5,
            restitution: 0.15,
            friction: 0.4,
            separation_percent: 0.85,
            max_impulse: 2500.0,
            damage_speed_threshold: 2.5,
            wall_restitution: 0.05,
            enable_walls: true,
            enable_car_car: true,
        }
    }
}

struct Obb {
    cx: f32,
    cz: f32,
    hx: f32,
    hz: f32,
    wx: f32,
    wz: f32,
    half_l: f32,
    half_w: f32,
}

fn make_obb(b: &PitCarBody) -> Obb {
    Obb {
        cx: b.x,
        cz: b.z,
        hx: b.heading.sin(),
        hz: b.heading.cos(),
        wx: (b.heading + std::f32::consts::FRAC_PI_2).sin(),
        wz: (b.heading + std::f32::consts::FRAC_PI_2).cos(),
        half_l: b.length * 0.5,
        half_w: b.width * 0.5,
    }
}

/// SAT: ritorna `(nx, nz, pen)` se i due OBB si sovrappongono.
fn overlap_obb(a: &Obb, b: &Obb) -> Option<(f32, f32, f32)> {
    let dx = a.cx - b.cx;
    let dz = a.cz - b.cz;

    let mut min_pen = 1e9_f32;
    let mut best_nx = 0.0_f32;
    let mut best_nz = 0.0_f32;

    let mut test_axis = |ax: f32, az: f32| -> bool {
        let r_a =
            a.half_l * (a.hx * ax + a.hz * az).abs() + a.half_w * (a.wx * ax + a.wz * az).abs();
        let r_b =
            b.half_l * (b.hx * ax + b.hz * az).abs() + b.half_w * (b.wx * ax + b.wz * az).abs();
        let dist = (dx * ax + dz * az).abs();
        let overlap = r_a + r_b - dist;
        if overlap <= 0.0 {
            return false;
        }
        if overlap < min_pen {
            min_pen = overlap;
            let side = if dx * ax + dz * az >= 0.0 { 1.0 } else { -1.0 };
            best_nx = ax * side;
            best_nz = az * side;
        }
        true
    };

    if !test_axis(a.hx, a.hz) {
        return None;
    }
    if !test_axis(a.wx, a.wz) {
        return None;
    }
    if !test_axis(b.hx, b.hz) {
        return None;
    }
    if !test_axis(b.wx, b.wz) {
        return None;
    }

    if min_pen > 0.0 {
        Some((best_nx, best_nz, min_pen))
    } else {
        None
    }
}

fn resolve_pair(
    a: &mut PitCarBody,
    b: &mut PitCarBody,
    nx: f32,
    nz: f32,
    pen: f32,
    cfg: &PitLaneCollisionConfig,
) -> Option<PitCollisionContact> {
    if a.invulnerable && b.invulnerable {
        return None;
    }

    let inv_mass_a = if a.invulnerable {
        0.0
    } else {
        1.0 / a.mass.max(1.0)
    };
    let inv_mass_b = if b.invulnerable {
        0.0
    } else {
        1.0 / b.mass.max(1.0)
    };
    let inv_sum = inv_mass_a + inv_mass_b;
    if inv_sum < 1e-8 {
        return None;
    }

    let corr = pen * cfg.separation_percent / inv_sum;
    a.x += nx * corr * inv_mass_a;
    a.z += nz * corr * inv_mass_a;
    b.x -= nx * corr * inv_mass_b;
    b.z -= nz * corr * inv_mass_b;

    let rvx = a.vx - b.vx;
    let rvz = a.vz - b.vz;
    let vel_along = rvx * nx + rvz * nz;

    let mut c = PitCollisionContact {
        car_a: a.car_id,
        car_b: b.car_id,
        nx,
        nz,
        penetration: pen,
        rel_speed: (-vel_along).max(0.0),
        ..Default::default()
    };

    if vel_along < 0.0 {
        let e = cfg.restitution;
        let mut j = -(1.0 + e) * vel_along / inv_sum;
        j = j.min(cfg.max_impulse);
        a.vx += j * inv_mass_a * nx;
        a.vz += j * inv_mass_a * nz;
        b.vx -= j * inv_mass_b * nx;
        b.vz -= j * inv_mass_b * nz;

        let mut tx = rvx - vel_along * nx;
        let mut tz = rvz - vel_along * nz;
        let tlen = (tx * tx + tz * tz).sqrt();
        if tlen > 1e-4 {
            tx /= tlen;
            tz /= tlen;
            let jt = -cfg.friction * j;
            a.vx += jt * inv_mass_a * tx;
            a.vz += jt * inv_mass_a * tz;
            b.vx -= jt * inv_mass_b * tx;
            b.vz -= jt * inv_mass_b * tz;
        }
        c.impulse = j;
    }

    Some(c)
}

/// Callback su un contatto di collisione.
type ContactCallback = Box<dyn FnMut(&PitCollisionContact)>;

/// Collisioni a bassa velocità in corsia (port di `PitLaneCollision`).
pub struct PitLaneCollision {
    cfg: PitLaneCollisionConfig,
    axis: PitAxis,
    bodies: Vec<PitCarBody>,
    contacts: Vec<PitCollisionContact>,
    on_contact: Option<ContactCallback>,
}

impl Default for PitLaneCollision {
    fn default() -> Self {
        Self::new()
    }
}

impl PitLaneCollision {
    /// Un sistema di collisioni vuoto.
    pub fn new() -> Self {
        Self {
            cfg: PitLaneCollisionConfig::default(),
            axis: PitAxis::default(),
            bodies: Vec::new(),
            contacts: Vec::new(),
            on_contact: None,
        }
    }

    /// Imposta la configurazione.
    pub fn set_config(&mut self, cfg: PitLaneCollisionConfig) {
        self.cfg = cfg;
    }

    /// La configurazione.
    pub fn config(&self) -> &PitLaneCollisionConfig {
        &self.cfg
    }

    /// Imposta l'asse della corsia.
    pub fn set_axis(&mut self, axis: PitAxis) {
        self.axis = axis;
    }

    /// I corpi.
    pub fn bodies(&self) -> &[PitCarBody] {
        &self.bodies
    }

    /// I corpi (mutabili).
    pub fn bodies_mut(&mut self) -> &mut [PitCarBody] {
        &mut self.bodies
    }

    /// Gli ultimi contatti.
    pub fn last_contacts(&self) -> &[PitCollisionContact] {
        &self.contacts
    }

    /// Registra la callback sui contatti.
    pub fn set_on_contact(&mut self, callback: impl FnMut(&PitCollisionContact) + 'static) {
        self.on_contact = Some(Box::new(callback));
    }

    /// Trova un corpo per id.
    pub fn find(&self, car_id: i32) -> Option<&PitCarBody> {
        self.bodies.iter().find(|b| b.car_id == car_id)
    }

    /// Inserisce o sostituisce un corpo.
    pub fn upsert(&mut self, body: PitCarBody) {
        if let Some(b) = self.bodies.iter_mut().find(|b| b.car_id == body.car_id) {
            *b = body;
            return;
        }
        self.bodies.push(body);
    }

    /// Rimuove un corpo.
    pub fn remove(&mut self, car_id: i32) {
        self.bodies.retain(|b| b.car_id != car_id);
    }

    /// Esegue coppie + muri, aggiornando posizioni e velocità.
    pub fn step(&mut self, _dt: f32) -> usize {
        self.contacts.clear();
        if self.cfg.enable_car_car {
            self.collide_pairs();
        }
        if self.cfg.enable_walls {
            self.collide_walls();
        }
        self.contacts.len()
    }

    /// Impulso di danno per un'auto (0 se morbido).
    pub fn damage_impulse_for(&self, car_id: i32) -> f32 {
        let mut best = 0.0_f32;
        for c in &self.contacts {
            if c.car_a != car_id && c.car_b != car_id {
                continue;
            }
            if c.rel_speed < self.cfg.damage_speed_threshold {
                continue;
            }
            best = best.max(c.impulse);
        }
        best
    }

    /// L'auto è in contatto.
    pub fn is_in_contact(&self, car_id: i32) -> bool {
        self.contacts
            .iter()
            .any(|c| c.car_a == car_id || c.car_b == car_id)
    }

    /// Aggancio alla coda (no-op, come nell'originale).
    pub fn apply_to_queue(&self, _queue: &mut PitLaneQueue) {}

    fn fire_contacts(&mut self, contacts: Vec<PitCollisionContact>) {
        for c in &contacts {
            if let Some(cb) = self.on_contact.as_mut() {
                cb(c);
            }
        }
        self.contacts.extend(contacts);
    }

    fn collide_pairs(&mut self) {
        let cfg = self.cfg;
        let mut contacts = Vec::new();
        let n = self.bodies.len();
        for i in 0..n {
            for j in (i + 1)..n {
                if !self.bodies[i].active || !self.bodies[j].active {
                    continue;
                }
                let oa = make_obb(&self.bodies[i]);
                let ob = make_obb(&self.bodies[j]);
                let Some((nx, nz, pen)) = overlap_obb(&oa, &ob) else {
                    continue;
                };
                let (left, right) = self.bodies.split_at_mut(j);
                let a = &mut left[i];
                let b = &mut right[0];
                if let Some(c) = resolve_pair(a, b, nx, nz, pen, &cfg) {
                    contacts.push(c);
                }
            }
        }
        self.fire_contacts(contacts);
    }

    fn collide_walls(&mut self) {
        let cfg = self.cfg;
        let axis = self.axis;
        let hx = axis.heading.sin();
        let hz = axis.heading.cos();
        let lx = (axis.heading + std::f32::consts::FRAC_PI_2).sin();
        let lz = (axis.heading + std::f32::consts::FRAC_PI_2).cos();
        let half = cfg.pit_half_width_m;

        let mut contacts = Vec::new();
        for b in &mut self.bodies {
            if !b.active || b.invulnerable {
                continue;
            }

            let dx = b.x - axis.origin_x;
            let dz = b.z - axis.origin_z;
            let lat = dx * lx + dz * lz;
            let limit = half - b.width * 0.5;
            if limit <= 0.1 {
                continue;
            }

            if lat > limit || lat < -limit {
                let over = if lat > 0.0 { lat - limit } else { lat + limit };
                let sign = if lat > 0.0 { 1.0 } else { -1.0 };
                let nx = -lx * sign;
                let nz = -lz * sign;
                let pen = over.abs();

                b.x += nx * pen * cfg.separation_percent;
                b.z += nz * pen * cfg.separation_percent;

                let vel_along = b.vx * nx + b.vz * nz;
                let mut j = 0.0_f32;
                if vel_along < 0.0 {
                    j = -(1.0 + cfg.wall_restitution) * vel_along;
                    j = j.min(cfg.max_impulse * 0.5);
                    b.vx += j * nx;
                    b.vz += j * nz;
                }

                contacts.push(PitCollisionContact {
                    car_a: b.car_id,
                    car_b: -1,
                    nx,
                    nz,
                    penetration: pen,
                    rel_speed: (-vel_along).max(0.0),
                    impulse: j,
                    with_wall: true,
                });
            }

            let _ = (hx, hz);
        }
        self.fire_contacts(contacts);
    }
}

/// Sincronizza i corpi delle collisioni dalle voci della coda (port di
/// `syncPitCollisionFromQueue`).
pub fn sync_pit_collision_from_queue(
    col: &mut PitLaneCollision,
    queue: &PitLaneQueue,
    default_heading: f32,
) {
    for e in queue.entries() {
        let mut b = col.find(e.car_id).copied().unwrap_or_default();
        b.car_id = e.car_id;
        b.x = e.pos_x;
        b.z = e.pos_z;
        b.vx = e.speed_ms * default_heading.sin();
        b.vz = e.speed_ms * default_heading.cos();
        b.heading = default_heading;
        b.active = e.status != PitQueueStatus::Done;
        b.invulnerable = e.role == PitQueueRole::Holding;
        col.upsert(b);
    }
}

/// Riporta i risultati delle collisioni nella coda (port di
/// `applyPitCollisionResults`).
pub fn apply_pit_collision_results(
    col: &mut PitLaneCollision,
    queue: &mut PitLaneQueue,
    mut set_pose: impl FnMut(i32, f32, f32, f32, f32),
) {
    for b in col.bodies() {
        queue.update_car(b.car_id, b.x, b.z, (b.vx * b.vx + b.vz * b.vz).sqrt());
        set_pose(b.car_id, b.x, b.z, b.vx, b.vz);
    }
    col.apply_to_queue(queue);
}

// ── Servizio e riparazioni (PitLaneRepair) ──────────────────────────────────

/// Tipo di lavoro in piazzola (port di `PitJobType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PitJobType {
    /// Carrozzeria.
    #[default]
    Body,
    /// Sospensioni.
    Suspension,
    /// Aerodinamica.
    Aero,
    /// Motore.
    Engine,
    /// Trasmissione.
    Transmission,
    /// Freni.
    Brakes,
    /// Pneumatici.
    Tyres,
    /// Carburante.
    Fuel,
    /// Servizio completo.
    FullService,
}

/// Stato di un lavoro (port di `PitJobState`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PitJobState {
    /// In coda.
    #[default]
    Queued,
    /// In corso.
    Running,
    /// Concluso.
    Done,
    /// Annullato.
    Cancelled,
}

/// Un lavoro in piazzola (port di `PitJob`).
#[derive(Clone, Copy, Debug)]
pub struct PitJob {
    /// Tipo.
    pub job_type: PitJobType,
    /// Stato.
    pub state: PitJobState,
    /// Durata (s).
    pub duration_sec: f32,
    /// Tempo trascorso (s).
    pub elapsed_sec: f32,
    /// Avanzamento (0..1).
    pub progress: f32,
    /// Carburante da aggiungere (L).
    pub fuel_add_l: f32,
    /// Cambia gli pneumatici.
    pub change_tyres: bool,
}

impl Default for PitJob {
    fn default() -> Self {
        Self {
            job_type: PitJobType::Body,
            state: PitJobState::Queued,
            duration_sec: 1.0,
            elapsed_sec: 0.0,
            progress: 0.0,
            fuel_add_l: 0.0,
            change_tyres: false,
        }
    }
}

/// Configurazione del servizio (port di `PitRepairConfig`).
#[derive(Clone, Copy, Debug)]
pub struct PitRepairConfig {
    /// Scala tempo carrozzeria.
    pub body_time_scale: f32,
    /// Scala tempo sospensioni.
    pub susp_time_scale: f32,
    /// Scala tempo aerodinamica.
    pub aero_time_scale: f32,
    /// Scala tempo motore.
    pub engine_time_scale: f32,
    /// Scala tempo trasmissione.
    pub trans_time_scale: f32,
    /// Scala tempo freni.
    pub brakes_time_scale: f32,
    /// Durata cambio gomme (s).
    pub tyre_change_sec: f32,
    /// Velocità di rifornimento (L/s).
    pub fuel_rate_lps: f32,
    /// Velocità massima per considerare l'auto ferma (m/s).
    pub min_stationary_speed_ms: f32,
    /// Rifornimento massimo (L).
    pub max_refuel_l: f32,
    /// Lavori in parallelo.
    pub parallel_jobs: bool,
    /// Numero massimo di lavori paralleli.
    pub max_parallel: i32,
    /// Tempo di riparazione per uno stop-and-go (s).
    pub stop_go_repair_sec: f32,
}

impl Default for PitRepairConfig {
    fn default() -> Self {
        Self {
            body_time_scale: 1.0,
            susp_time_scale: 1.0,
            aero_time_scale: 1.0,
            engine_time_scale: 1.0,
            trans_time_scale: 1.0,
            brakes_time_scale: 1.0,
            tyre_change_sec: 4.5,
            fuel_rate_lps: 2.5,
            min_stationary_speed_ms: 0.35,
            max_refuel_l: 120.0,
            parallel_jobs: true,
            max_parallel: 2,
            stop_go_repair_sec: 10.0,
        }
    }
}

/// Ingresso del servizio (port di `PitRepairInput`).
#[derive(Clone, Copy, Debug)]
pub struct PitRepairInput {
    /// In box.
    pub in_garage_box: bool,
    /// Servizio richiesto.
    pub request_service: bool,
    /// Servizio annullato.
    pub request_abort: bool,
    /// Serve uno stop-and-go.
    pub serve_stop_go: bool,
    /// Velocità (m/s).
    pub speed_ms: f32,
    /// Carburante attuale (L).
    pub fuel_l: f32,
    /// Capacità serbatoio (L).
    pub fuel_capacity_l: f32,
    /// Carburante obiettivo (L), -1 = nessuno.
    pub target_fuel_l: f32,
    /// Cambiare gomme.
    pub want_tyres: bool,
    /// Riparare carrozzeria.
    pub want_body: bool,
    /// Riparare sospensioni.
    pub want_suspension: bool,
    /// Riparare aerodinamica.
    pub want_aero: bool,
    /// Riparare motore.
    pub want_engine: bool,
    /// Riparare trasmissione.
    pub want_transmission: bool,
    /// Riparare freni.
    pub want_brakes: bool,
    /// Servizio completo.
    pub want_full: bool,
}

impl Default for PitRepairInput {
    fn default() -> Self {
        Self {
            in_garage_box: false,
            request_service: false,
            request_abort: false,
            serve_stop_go: false,
            speed_ms: 0.0,
            fuel_l: 50.0,
            fuel_capacity_l: 100.0,
            target_fuel_l: -1.0,
            want_tyres: false,
            want_body: true,
            want_suspension: true,
            want_aero: true,
            want_engine: false,
            want_transmission: false,
            want_brakes: false,
            want_full: false,
        }
    }
}

/// Uscita del servizio (port di `PitRepairOutput`).
#[derive(Clone, Debug, Default)]
pub struct PitRepairOutput {
    /// Servizio attivo.
    pub service_active: bool,
    /// Trattiene l'auto.
    pub hold_car: bool,
    /// Un lavoro è stato completato in questo frame.
    pub completed_this_frame: bool,
    /// Avanzamento complessivo.
    pub overall_progress: f32,
    /// Tempo rimanente (s).
    pub remaining_sec: f32,
    /// Carburante (L).
    pub fuel_l: f32,
    /// Gomme cambiate.
    pub tyres_changed: bool,
    /// Testo di stato.
    pub status_text: String,
    /// Lavori conclusi.
    pub jobs_done: i32,
    /// Lavori totali.
    pub jobs_total: i32,
}

/// Servizio e riparazioni in piazzola (port di `PitLaneRepair`).
pub struct PitLaneRepair {
    cfg: PitRepairConfig,
    jobs: Vec<PitJob>,
    busy: bool,
    tyres_done: bool,
    fuel_l: f32,
    fuel_cap: f32,
    on_service_complete: Option<Box<dyn FnMut()>>,
}

impl Default for PitLaneRepair {
    fn default() -> Self {
        Self::new()
    }
}

impl PitLaneRepair {
    /// Un sistema di servizio vuoto.
    pub fn new() -> Self {
        Self {
            cfg: PitRepairConfig::default(),
            jobs: Vec::new(),
            busy: false,
            tyres_done: false,
            fuel_l: 0.0,
            fuel_cap: 100.0,
            on_service_complete: None,
        }
    }

    /// Imposta la configurazione.
    pub fn set_config(&mut self, cfg: PitRepairConfig) {
        self.cfg = cfg;
    }

    /// La configurazione.
    pub fn config(&self) -> &PitRepairConfig {
        &self.cfg
    }

    /// Registra la callback di completamento.
    pub fn set_on_service_complete(&mut self, callback: impl FnMut() + 'static) {
        self.on_service_complete = Some(Box::new(callback));
    }

    /// Il servizio è in corso.
    pub fn is_busy(&self) -> bool {
        self.busy
    }

    /// I lavori.
    pub fn jobs(&self) -> &[PitJob] {
        &self.jobs
    }

    /// Pianifica i lavori in base ai danni (port di `plan`).
    pub fn plan(&mut self, dmg: &DamageSystem, input: &PitRepairInput) {
        self.jobs.clear();
        self.busy = false;
        self.tyres_done = false;
        self.fuel_l = input.fuel_l;
        self.fuel_cap = input.fuel_capacity_l;

        let cfg = self.cfg;
        let rd = dmg.calculate_repair_data();

        if input.serve_stop_go {
            self.add_job(PitJobType::Body, cfg.stop_go_repair_sec);
        } else if input.want_full {
            let body = rd.body_repair_time * cfg.body_time_scale * 0.5;
            let sus = rd.suspension_repair_time * cfg.susp_time_scale * 0.5;
            let aero = rd.aero_repair_time * cfg.aero_time_scale * 0.5;
            self.add_job(
                PitJobType::FullService,
                body.max(sus).max(aero).max(cfg.tyre_change_sec) + 1.0,
            );
            if let Some(j) = self.jobs.last_mut() {
                j.change_tyres = true;
            }
            let target = if input.target_fuel_l >= 0.0 {
                input.target_fuel_l.min(self.fuel_cap)
            } else {
                self.fuel_cap
            };
            let fuel_add = (target - input.fuel_l).clamp(0.0, cfg.max_refuel_l);
            if let Some(j) = self.jobs.last_mut() {
                j.fuel_add_l = fuel_add;
            }
        } else {
            if input.want_body && rd.body_repair_time > 0.2 {
                self.add_job(PitJobType::Body, rd.body_repair_time * cfg.body_time_scale);
            }
            if input.want_suspension && rd.suspension_repair_time > 0.2 {
                self.add_job(
                    PitJobType::Suspension,
                    rd.suspension_repair_time * cfg.susp_time_scale,
                );
            }
            if input.want_aero && rd.aero_repair_time > 0.2 {
                self.add_job(PitJobType::Aero, rd.aero_repair_time * cfg.aero_time_scale);
            }
            if input.want_engine && rd.engine_repair_time > 0.2 {
                self.add_job(
                    PitJobType::Engine,
                    rd.engine_repair_time * cfg.engine_time_scale,
                );
            }
            if input.want_transmission {
                self.add_job(PitJobType::Transmission, 25.0 * cfg.trans_time_scale);
            }
            if input.want_brakes {
                self.add_job(PitJobType::Brakes, 12.0 * cfg.brakes_time_scale);
            }
            if input.want_tyres {
                self.add_job(PitJobType::Tyres, cfg.tyre_change_sec);
                if let Some(j) = self.jobs.last_mut() {
                    j.change_tyres = true;
                }
            }
            let target = if input.target_fuel_l >= 0.0 {
                input.target_fuel_l.min(self.fuel_cap)
            } else {
                -1.0
            };
            if target >= 0.0 {
                let need = (target - input.fuel_l).clamp(0.0, cfg.max_refuel_l);
                if need > 0.05 {
                    self.add_job(PitJobType::Fuel, need / cfg.fuel_rate_lps.max(0.1));
                    if let Some(j) = self.jobs.last_mut() {
                        j.fuel_add_l = need;
                    }
                }
            }
        }
    }

    /// Avanza il servizio (port di `update`).
    pub fn update(
        &mut self,
        dt: f32,
        input: &PitRepairInput,
        dmg: &mut DamageSystem,
        garage_phase: GarageExitPhase,
    ) -> PitRepairOutput {
        let mut out = PitRepairOutput {
            fuel_l: self.fuel_l,
            ..Default::default()
        };

        let can_service = (input.in_garage_box
            || garage_phase == GarageExitPhase::InGarage
            || garage_phase == GarageExitPhase::Returning)
            && input.speed_ms <= self.cfg.min_stationary_speed_ms;

        if input.request_abort && self.busy {
            for j in &mut self.jobs {
                if j.state != PitJobState::Done {
                    j.state = PitJobState::Cancelled;
                }
            }
            self.busy = false;
            out.status_text = "SERVICE ABORTED".to_string();
            return out;
        }

        if !self.busy && input.request_service && can_service {
            self.plan(dmg, input);
            if self.jobs.is_empty() {
                out.status_text = "NO WORK".to_string();
                return out;
            }
            self.busy = true;
            let limit = self.parallel_limit();
            let mut started = 0;
            for j in &mut self.jobs {
                if j.state != PitJobState::Queued {
                    continue;
                }
                j.state = PitJobState::Running;
                started += 1;
                if started >= limit {
                    break;
                }
            }
        }

        if !self.busy {
            out.status_text = if can_service {
                "READY FOR SERVICE".to_string()
            } else {
                "ENTER GARAGE".to_string()
            };
            return out;
        }

        if !can_service {
            out.service_active = true;
            out.hold_car = true;
            out.status_text = "HOLD — MUST BE STATIONARY IN BOX".to_string();
            return out;
        }

        out.service_active = true;
        out.hold_car = true;

        let mut running = 0;
        let mut done = 0;
        let mut rem = 0.0_f32;
        let mut prog_sum = 0.0_f32;

        for i in 0..self.jobs.len() {
            if self.jobs[i].state == PitJobState::Running {
                self.jobs[i].elapsed_sec += dt;
                let dur = self.jobs[i].duration_sec;
                self.jobs[i].progress = (self.jobs[i].elapsed_sec / dur).clamp(0.0, 1.0);
                if self.jobs[i].job_type == PitJobType::Fuel && self.jobs[i].fuel_add_l > 0.0 {
                    let add = self.cfg.fuel_rate_lps * dt;
                    let room = self.fuel_cap - self.fuel_l;
                    let got = add.min(room).min(self.jobs[i].fuel_add_l);
                    self.fuel_l += got;
                    self.jobs[i].fuel_add_l -= got;
                }
                if self.jobs[i].elapsed_sec >= dur {
                    Self::complete_job(&self.jobs[i], dmg, &mut self.tyres_done);
                    self.jobs[i].state = PitJobState::Done;
                    self.jobs[i].progress = 1.0;
                    out.completed_this_frame = true;
                } else {
                    running += 1;
                    rem += dur - self.jobs[i].elapsed_sec;
                }
            }
            if self.jobs[i].state == PitJobState::Done {
                done += 1;
            }
            prog_sum += self.jobs[i].progress;
        }

        while running < self.parallel_limit() {
            let mut started = false;
            for j in &mut self.jobs {
                if j.state != PitJobState::Queued {
                    continue;
                }
                j.state = PitJobState::Running;
                running += 1;
                started = true;
                break;
            }
            if !started {
                break;
            }
        }

        out.jobs_done = done;
        out.jobs_total = self.jobs.len() as i32;
        out.overall_progress = if self.jobs.is_empty() {
            1.0
        } else {
            prog_sum / self.jobs.len() as f32
        };
        out.remaining_sec = rem;
        out.fuel_l = self.fuel_l;
        out.tyres_changed = self.tyres_done;

        if done >= out.jobs_total {
            self.busy = false;
            out.service_active = false;
            out.hold_car = false;
            out.overall_progress = 1.0;
            out.status_text = "SERVICE COMPLETE".to_string();
            out.completed_this_frame = true;
            if let Some(cb) = self.on_service_complete.as_mut() {
                cb();
            }
        } else {
            out.status_text = self.status_from_jobs();
        }
        out
    }

    /// Nome leggibile di un lavoro (port di `jobName`).
    pub fn job_name(job_type: PitJobType) -> &'static str {
        match job_type {
            PitJobType::Body => "BODY",
            PitJobType::Suspension => "SUSPENSION",
            PitJobType::Aero => "AERO",
            PitJobType::Engine => "ENGINE",
            PitJobType::Transmission => "GEARBOX",
            PitJobType::Brakes => "BRAKES",
            PitJobType::Tyres => "TYRES",
            PitJobType::Fuel => "FUEL",
            PitJobType::FullService => "FULL SERVICE",
        }
    }

    fn parallel_limit(&self) -> i32 {
        if self.cfg.parallel_jobs {
            self.cfg.max_parallel.max(1)
        } else {
            1
        }
    }

    fn add_job(&mut self, job_type: PitJobType, duration_sec: f32) {
        self.jobs.push(PitJob {
            job_type,
            duration_sec: duration_sec.max(0.05),
            ..Default::default()
        });
    }

    fn complete_job(job: &PitJob, dmg: &mut DamageSystem, tyres_done: &mut bool) {
        match job.job_type {
            PitJobType::Body => dmg.repair_partial(0.85),
            PitJobType::Suspension => dmg.repair_system(DamageType::SUSPENSION),
            PitJobType::Aero => dmg.repair_system(DamageType::AERO),
            PitJobType::Engine => dmg.repair_system(DamageType::ENGINE),
            PitJobType::Transmission => dmg.repair_system(DamageType::TRANSMISSION),
            PitJobType::Brakes => dmg.repair_system(DamageType::BRAKES),
            PitJobType::Tyres => *tyres_done = true,
            PitJobType::Fuel => {}
            PitJobType::FullService => {
                dmg.repair_partial(0.75);
                dmg.repair_system(DamageType::SUSPENSION);
                dmg.repair_system(DamageType::AERO);
                *tyres_done = job.change_tyres;
            }
        }
    }

    fn status_from_jobs(&self) -> String {
        for j in &self.jobs {
            if j.state == PitJobState::Running {
                return format!("{} {:.0}%", Self::job_name(j.job_type), j.progress * 100.0);
            }
        }
        "SERVICING".to_string()
    }
}

/// Stato della strategia ai box (port di `PitStrategyState`).
#[derive(Clone, Copy, Debug)]
pub struct PitStrategyState {
    /// Carburante obiettivo (L).
    pub fuel_target_l: f32,
    /// Cambiare gomme.
    pub tyres: bool,
    /// Riparare carrozzeria.
    pub body: bool,
    /// Riparare sospensioni.
    pub suspension: bool,
    /// Riparare aerodinamica.
    pub aero: bool,
    /// Riparare motore.
    pub engine: bool,
}

impl Default for PitStrategyState {
    fn default() -> Self {
        Self {
            fuel_target_l: 60.0,
            tyres: true,
            body: true,
            suspension: true,
            aero: false,
            engine: false,
        }
    }
}

/// Mappa la strategia del menu sull'ingresso del servizio (port di
/// `applyStrategyToRepairInput`).
pub fn apply_strategy_to_repair_input(state: &PitStrategyState, input: &mut PitRepairInput) {
    input.target_fuel_l = state.fuel_target_l;
    input.want_tyres = state.tyres;
    input.want_body = state.body;
    input.want_suspension = state.suspension;
    input.want_aero = state.aero;
    input.want_engine = state.engine;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulator::damage::{CollisionEvent, DamageConfig, DamageSystem, DamageType};
    use bevy::math::Vec3;

    fn damage_car(dmg: &mut DamageSystem) {
        dmg.set_config(DamageConfig {
            impact_threshold: 1.0,
            ..Default::default()
        });
        dmg.process_collision(CollisionEvent {
            contact_point: Vec3::new(0.0, 0.4, 2.4),
            impact_energy: 60000.0,
            damage_type: DamageType::BODY_PANEL,
            ..Default::default()
        });
        dmg.process_collision(CollisionEvent {
            contact_point: Vec3::new(1.0, 0.35, 1.5),
            impact_energy: 40000.0,
            damage_type: DamageType::SUSPENSION,
            ..Default::default()
        });
    }

    #[test]
    fn garage_exit_blocks_in_locked_race() {
        let mut ctrl = GarageExitController::new();
        ctrl.bind_box(
            0,
            GarageBoxPose {
                x: 0.0,
                z: 0.0,
                heading: 0.0,
            },
            0.0,
        );
        let input = GarageExitInput {
            request_leave: true,
            engine_running: true,
            session: SessionType::Race,
            session_phase: SessionPhase::Green,
            ..Default::default()
        };
        let out = ctrl.update(0.016, &input);
        assert_eq!(out.phase, GarageExitPhase::Blocked);
        assert_eq!(out.block_reason, GarageBlockReason::SessionLocked);
    }

    #[test]
    fn garage_exit_reaches_track() {
        let mut ctrl = GarageExitController::new();
        ctrl.bind_box(
            0,
            GarageBoxPose {
                x: 0.0,
                z: 0.0,
                heading: 0.0,
            },
            0.0,
        );
        let mut input = GarageExitInput {
            request_leave: true,
            engine_running: true,
            session: SessionType::Practice,
            ..Default::default()
        };

        let mut phases = Vec::new();
        let mut limiter_seen = false;
        for _ in 0..2000 {
            let out = ctrl.update(0.016, &input);
            phases.push(out.phase);
            if out.pit_limiter_active {
                limiter_seen = true;
            }
            if out.phase == GarageExitPhase::OnTrack {
                break;
            }
            input.pos_z += 2.0;
        }
        assert_eq!(ctrl.phase(), GarageExitPhase::OnTrack);
        assert!(phases.contains(&GarageExitPhase::RollingOut));
        assert!(phases.contains(&GarageExitPhase::PitLane));
        assert!(limiter_seen);
    }

    #[test]
    fn pit_limiter_cuts_throttle_near_limit() {
        let max = 60.0 / 3.6;
        assert_eq!(GarageExitController::apply_pit_limiter(0.0, 1.0, max), 1.0);
        assert!((GarageExitController::apply_pit_limiter(max * 0.9, 1.0, max) - 0.35).abs() < 1e-6);
        assert!((GarageExitController::apply_pit_limiter(max, 1.0, max) - 0.05).abs() < 1e-6);
    }

    #[test]
    fn exit_path_blocked_detects_car_in_cone() {
        let box_pose = GarageBoxPose {
            x: 0.0,
            z: 0.0,
            heading: 0.0,
        };
        // Cone points along +Z (heading 0 -> sin=0, cos=1).
        let others = [(0.0_f32, 5.0_f32)];
        assert!(is_exit_path_blocked(
            &box_pose, 0.0, 15.0, 2.5, &others, 0.0, 0.0
        ));
        // A car behind the box is not in the cone.
        let behind = [(0.0_f32, -5.0_f32)];
        assert!(!is_exit_path_blocked(
            &box_pose, 0.0, 15.0, 2.5, &behind, 0.0, 0.0
        ));
        // The self car is ignored.
        assert!(!is_exit_path_blocked(
            &box_pose, 0.0, 15.0, 2.5, &others, 0.0, 5.0
        ));
    }

    #[test]
    fn queue_releases_second_car_after_gap() {
        let mut queue = PitLaneQueue::new();
        queue.set_axis(PitAxis {
            origin_x: 0.0,
            origin_z: 0.0,
            heading: 0.0,
        });
        queue.request_leave(1, 1, 0, false, 0.0, 0.0);
        queue.request_leave(2, 2, 1, false, 2.0, 0.0);
        queue.update(0.016);

        assert!(queue.is_cleared(1));
        assert!(queue.is_waiting(2));
        assert_eq!(queue.leave_queue_position(1), 0);
        assert!(queue.should_block_garage_exit(2));

        queue.update_car(1, 0.0, 20.0, 10.0);
        queue.update(0.016);
        assert!(queue.is_cleared(2));
        assert!(!queue.should_block_garage_exit(2));
    }

    #[test]
    fn queue_enter_spacing_blocks_close_cars() {
        let mut queue = PitLaneQueue::new();
        queue.set_axis(PitAxis {
            origin_x: 0.0,
            origin_z: 0.0,
            heading: 0.0,
        });
        queue.request_enter(1, 1, 0, false, 0.0, 0.0);
        queue.request_enter(2, 2, 1, false, 0.0, 5.0);
        queue.update(0.016);
        assert!(queue.is_waiting(1));
        assert!(queue.is_waiting(2));

        queue.update_car(2, 0.0, 30.0, 10.0);
        queue.update(0.016);
        assert!(queue.is_cleared(1));
        assert!(queue.is_cleared(2));
    }

    #[test]
    fn collision_separates_and_reports_impulse() {
        let mut col = PitLaneCollision::new();
        col.upsert(PitCarBody {
            car_id: 1,
            x: 0.0,
            z: 0.0,
            vz: 5.0,
            ..Default::default()
        });
        col.upsert(PitCarBody {
            car_id: 2,
            x: 0.0,
            z: 4.0,
            vz: 0.0,
            ..Default::default()
        });
        let contacts = col.step(0.016);
        assert_eq!(contacts, 1);
        assert!(col.is_in_contact(1));
        assert!(col.damage_impulse_for(1) > 0.0);
        // They must have been pushed apart.
        let a = col.find(1).unwrap();
        let b = col.find(2).unwrap();
        assert!((b.z - a.z) > 4.0);
    }

    #[test]
    fn wall_keeps_car_in_corridor() {
        let mut col = PitLaneCollision::new();
        col.set_axis(PitAxis {
            origin_x: 0.0,
            origin_z: 0.0,
            heading: 0.0,
        });
        col.upsert(PitCarBody {
            car_id: 1,
            x: 5.0,
            z: 10.0,
            ..Default::default()
        });
        col.step(0.016);
        let b = col.find(1).unwrap();
        assert!(b.x < 5.0);
        assert!(col.last_contacts().iter().any(|c| c.with_wall));
    }

    #[test]
    fn repair_plans_and_completes_jobs() {
        let mut dmg = DamageSystem::new();
        damage_car(&mut dmg);

        let mut repair = PitLaneRepair::new();
        let input = PitRepairInput {
            in_garage_box: true,
            request_service: true,
            want_tyres: true,
            target_fuel_l: 80.0,
            fuel_l: 30.0,
            fuel_capacity_l: 100.0,
            ..Default::default()
        };

        let mut completed = false;
        for _ in 0..20000 {
            let out = repair.update(0.05, &input, &mut dmg, GarageExitPhase::InGarage);
            if out.completed_this_frame && !out.service_active {
                completed = true;
                assert!(out.tyres_changed);
                assert!(out.fuel_l > 30.0);
                break;
            }
        }
        assert!(completed, "service never completed");
        assert!(!repair.is_busy());
        assert!(dmg.calculate_repair_data().body_repair_time < 60.0);
    }

    #[test]
    fn repair_aborts_on_request() {
        let mut dmg = DamageSystem::new();
        damage_car(&mut dmg);

        let mut repair = PitLaneRepair::new();
        let start = PitRepairInput {
            in_garage_box: true,
            request_service: true,
            fuel_l: 50.0,
            fuel_capacity_l: 100.0,
            ..Default::default()
        };
        repair.update(0.05, &start, &mut dmg, GarageExitPhase::InGarage);
        assert!(repair.is_busy());

        let abort = PitRepairInput {
            request_abort: true,
            ..start
        };
        let out = repair.update(0.05, &abort, &mut dmg, GarageExitPhase::InGarage);
        assert_eq!(out.status_text, "SERVICE ABORTED");
        assert!(!repair.is_busy());
    }

    #[test]
    fn strategy_maps_to_repair_input() {
        let state = PitStrategyState {
            fuel_target_l: 42.0,
            tyres: false,
            body: true,
            suspension: false,
            aero: true,
            engine: true,
        };
        let mut input = PitRepairInput::default();
        apply_strategy_to_repair_input(&state, &mut input);
        assert_eq!(input.target_fuel_l, 42.0);
        assert!(!input.want_tyres);
        assert!(input.want_body);
        assert!(!input.want_suspension);
        assert!(input.want_aero);
        assert!(input.want_engine);
    }

    #[test]
    fn sync_and_apply_round_trip() {
        let mut queue = PitLaneQueue::new();
        queue.set_axis(PitAxis {
            origin_x: 0.0,
            origin_z: 0.0,
            heading: 0.0,
        });
        queue.request_leave(1, 1, 0, false, 0.0, 0.0);

        let mut col = PitLaneCollision::new();
        sync_pit_collision_from_queue(&mut col, &queue, 0.0);
        assert!(col.find(1).is_some());
        col.step(0.016);

        let mut pushed = false;
        apply_pit_collision_results(&mut col, &mut queue, |_, _, _, _, _| pushed = true);
        assert!(pushed);
    }
}
