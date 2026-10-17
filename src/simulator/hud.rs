//! HUD di gara + overlay telemetria.
//!
//! Port di `src/simulator/DashboardOverlay.{h,cpp}`,
//! `src/simulator/TelemetryOverlay.{h,cpp}` e `src/simulator/ui/RaceTelemetryHud.h`
//! di ksengine. Il rendering Qt/`DrawList` diventa UI Bevy: la logica di layout e
//! formattazione resta nel modello puro (`RaceTelemetryHud::line`), così è
//! testabile, mentre il plugin Bevy si limita ad applicarla ai nodi di testo.

use std::collections::VecDeque;

use bevy::prelude::*;

use super::cockpit::CockpitTelemetry;

// =============================================================================
// MODELLO
// =============================================================================

/// Modalità del HUD di gara (port di `RaceHudMode`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RaceHudMode {
    Off = 0,
    Compact,
    #[default]
    Race,
    Engineer,
}

impl RaceHudMode {
    /// Modalità successiva nel ciclo (port di `cycleMode`).
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Compact,
            Self::Compact => Self::Race,
            Self::Race => Self::Engineer,
            Self::Engineer => Self::Off,
        }
    }

    /// Il HUD è disegnato in qualche modalità.
    pub fn is_visible(self) -> bool {
        self != Self::Off
    }
}

/// Campione telemetria per il HUD (port di `RaceHudSample`).
#[derive(Clone, Debug)]
pub struct RaceHudSample {
    pub speed_ms: f32,
    pub rpm: f32,
    pub max_rpm: f32,
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub steer: f32,
    pub lat_g: f32,
    pub lon_g: f32,
    pub fuel_l: f32,
    pub tyre_temp: [f32; 4],
    pub tyre_wear: [f32; 4],
    pub position: i32,
    pub total_cars: i32,
    pub lap: i32,
    pub total_laps: i32,
    pub current_time_ms: i32,
    pub last_time_ms: i32,
    pub best_time_ms: i32,
    pub sector: i32,
    pub sector_time_ms: [i32; 3],
    pub in_pit: bool,
    pub pit_limiter: bool,
    pub damage_overall: f32,
    pub engine_health: f32,
    pub power_mult: f32,
    pub damage_warning: i32,
    pub engine_seized: bool,
    /// Riga di stato decisa dal loop ("REFUEL 60 L", "IN GARAGE", ...).
    pub status_text: String,
}

impl Default for RaceHudSample {
    fn default() -> Self {
        Self {
            speed_ms: 0.0,
            rpm: 0.0,
            max_rpm: 8500.0,
            gear: 1,
            throttle: 0.0,
            brake: 0.0,
            steer: 0.0,
            lat_g: 0.0,
            lon_g: 0.0,
            fuel_l: 0.0,
            tyre_temp: [80.0; 4],
            tyre_wear: [0.0; 4],
            position: 1,
            total_cars: 1,
            lap: 1,
            total_laps: 0,
            current_time_ms: 0,
            last_time_ms: 0,
            best_time_ms: 0,
            sector: 0,
            sector_time_ms: [0; 3],
            in_pit: false,
            pit_limiter: false,
            damage_overall: 0.0,
            engine_health: 1.0,
            power_mult: 1.0,
            damage_warning: 0,
            engine_seized: false,
            status_text: String::new(),
        }
    }
}

/// Riga del HUD (le "posizioni" di `DrawList`).
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudElement {
    Position,
    Lap,
    Status,
    Speed,
    Gear,
    Rpm,
    GForce,
    Damage,
}

/// Formatta millisecondi come `m:ss.mmm` (port di `fmtTime`).
pub fn format_time_ms(ms: i32) -> String {
    if ms <= 0 {
        return "--:--.---".into();
    }
    format!("{}:{:02}.{:03}", ms / 60000, (ms / 1000) % 60, ms % 1000)
}

/// HUD di gara a modalità (port di `RaceTelemetryHud`).
#[derive(Clone, Debug, Default)]
pub struct RaceTelemetryHud {
    mode: RaceHudMode,
    sample: RaceHudSample,
    hist_speed: VecDeque<f32>,
    hist_thr: VecDeque<f32>,
    hist_brk: VecDeque<f32>,
    hist_rpm: VecDeque<f32>,
}

impl RaceTelemetryHud {
    /// Capacità delle code di storico (port di `kHist`).
    pub const HIST_SIZE: usize = 160;

    pub fn set_mode(&mut self, mode: RaceHudMode) {
        self.mode = mode;
    }

    pub fn mode(&self) -> RaceHudMode {
        self.mode
    }

    pub fn is_visible(&self) -> bool {
        self.mode.is_visible()
    }

    pub fn cycle_mode(&mut self) {
        self.mode = self.mode.next();
    }

    pub fn sample(&self) -> &RaceHudSample {
        &self.sample
    }

    /// Riga di stato, settata dal loop di simulazione.
    pub fn set_status(&mut self, text: impl Into<String>) {
        self.sample.status_text = text.into();
    }

    /// Spinge un campione e aggiorna lo storico (port di `push`).
    pub fn push(&mut self, sample: RaceHudSample) {
        push_hist(&mut self.hist_speed, sample.speed_ms * 3.6);
        push_hist(&mut self.hist_thr, sample.throttle);
        push_hist(&mut self.hist_brk, sample.brake);
        push_hist(&mut self.hist_rpm, sample.rpm / sample.max_rpm.max(1.0));
        self.sample = sample;
    }

    pub fn speed_history(&self) -> &VecDeque<f32> {
        &self.hist_speed
    }

    pub fn throttle_history(&self) -> &VecDeque<f32> {
        &self.hist_thr
    }

    pub fn brake_history(&self) -> &VecDeque<f32> {
        &self.hist_brk
    }

    pub fn rpm_history(&self) -> &VecDeque<f32> {
        &self.hist_rpm
    }

    /// Delta sull'ultimo giro rispetto al migliore (port di `deltaMs`).
    pub fn delta_ms(&self) -> i32 {
        if self.sample.last_time_ms > 0 && self.sample.best_time_ms > 0 {
            self.sample.last_time_ms - self.sample.best_time_ms
        } else {
            0
        }
    }

    /// Frazione rpm normalizzata 0..1.
    pub fn rpm_normalized(&self) -> f32 {
        (self.sample.rpm / self.sample.max_rpm.max(1.0)).clamp(0.0, 1.0)
    }

    /// Testo di una riga per la modalità corrente; `None` = nascosta.
    pub fn line(&self, el: HudElement) -> Option<String> {
        use HudElement as E;
        let s = &self.sample;
        let shown = match self.mode {
            RaceHudMode::Off => return None,
            RaceHudMode::Compact => matches!(el, E::Position | E::Status | E::Speed | E::Damage),
            RaceHudMode::Race => matches!(
                el,
                E::Position | E::Lap | E::Status | E::Speed | E::Gear | E::Rpm
            ),
            RaceHudMode::Engineer => matches!(
                el,
                E::Position
                    | E::Lap
                    | E::Status
                    | E::Speed
                    | E::Gear
                    | E::Rpm
                    | E::GForce
                    | E::Damage
            ),
        };
        if !shown {
            return None;
        }
        let text = match el {
            E::Position => {
                if self.mode == RaceHudMode::Compact {
                    format!("P{}", s.position)
                } else {
                    format!("P{}/{}", s.position, s.total_cars.max(1))
                }
            }
            E::Lap => {
                if s.total_laps > 0 {
                    format!("LAP {}/{}", s.lap, s.total_laps)
                } else {
                    format!("LAP {}", s.lap)
                }
            }
            E::Status => s.status_text.clone(),
            E::Speed => format!("{}", (s.speed_ms * 3.6 + 0.5) as i32),
            E::Gear => format!("{}", s.gear),
            E::Rpm => format!("RPM {} / {}", s.rpm as i32, s.max_rpm as i32),
            E::GForce => format!("G lat {:+.2}  lon {:+.2}", s.lat_g, s.lon_g),
            E::Damage => format!(
                "DMG {:.0}%  ENG {:.0}%  PWR {:.0}%{}",
                s.damage_overall * 100.0,
                s.engine_health * 100.0,
                s.power_mult * 100.0,
                if s.engine_seized { "  SEIZED" } else { "" }
            ),
        };
        if el == E::Status && text.is_empty() {
            return None;
        }
        if el == E::Damage && self.mode == RaceHudMode::Compact && s.damage_warning <= 0 {
            return None;
        }
        Some(text)
    }
}

fn push_hist(q: &mut VecDeque<f32>, v: f32) {
    q.push_back(v);
    while q.len() > RaceTelemetryHud::HIST_SIZE {
        q.pop_front();
    }
}

/// Overlay cruscotto semplice (port di `DashboardOverlay`).
#[derive(Clone, Debug)]
pub struct DashboardOverlay {
    pub visible: bool,
    pub opacity: f32,
    pub speed: f32,
    pub rpm: f32,
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub current_lap: i32,
    pub lap_time: f32,
    pub best_lap_time: f32,
    pub last_lap_time: f32,
    pub position: i32,
    pub total_cars: i32,
}

impl Default for DashboardOverlay {
    fn default() -> Self {
        Self {
            visible: true,
            opacity: 0.9,
            speed: 0.0,
            rpm: 0.0,
            gear: 0,
            throttle: 0.0,
            brake: 0.0,
            current_lap: 0,
            lap_time: 0.0,
            best_lap_time: 1e9,
            last_lap_time: 0.0,
            position: 1,
            total_cars: 1,
        }
    }
}

/// Dati per `DashboardOverlay::update`.
#[derive(Clone, Copy, Debug, Default)]
pub struct DashboardData {
    pub speed: f32,
    pub rpm: f32,
    pub gear: i32,
    pub throttle: f32,
    pub brake: f32,
    pub current_lap: i32,
    pub lap_time: f32,
    pub best_lap_time: f32,
    pub last_lap_time: f32,
    pub position: i32,
    pub total_cars: i32,
}

impl DashboardOverlay {
    pub fn set_visible(&mut self, v: bool) {
        self.visible = v;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_opacity(&mut self, o: f32) {
        self.opacity = o;
    }

    pub fn update(&mut self, d: DashboardData) {
        self.speed = d.speed;
        self.rpm = d.rpm;
        self.gear = d.gear;
        self.throttle = d.throttle;
        self.brake = d.brake;
        self.current_lap = d.current_lap;
        self.lap_time = d.lap_time;
        self.best_lap_time = d.best_lap_time;
        self.last_lap_time = d.last_lap_time;
        self.position = d.position;
        self.total_cars = d.total_cars;
    }

    /// Formatta secondi come `m:ss.mmm` (port di `formatTime`).
    pub fn format_time(seconds: f32) -> String {
        if !seconds.is_finite() || seconds < 0.0 || seconds >= 1e8 {
            return "--:--.---".into();
        }
        let total_ms = (seconds * 1000.0).round() as i64;
        format!(
            "{}:{:02}.{:03}",
            total_ms / 60000,
            (total_ms / 1000) % 60,
            total_ms % 1000
        )
    }
}

/// Overlay telemetria con storico (port di `TelemetryOverlay`).
#[derive(Clone, Debug)]
pub struct TelemetryOverlay {
    pub visible: bool,
    pub opacity: f32,
    pub speed: f32,
    pub rpm: f32,
    pub throttle: f32,
    pub brake: f32,
    pub steering: f32,
    pub lateral_g: f32,
    pub longitudinal_g: f32,
    pub speed_history: Vec<f32>,
    pub rpm_history: Vec<f32>,
    pub throttle_history: Vec<f32>,
    pub brake_history: Vec<f32>,
    pub steering_history: Vec<f32>,
    pub lateral_g_history: Vec<f32>,
    pub longitudinal_g_history: Vec<f32>,
}

impl Default for TelemetryOverlay {
    fn default() -> Self {
        Self {
            visible: false,
            opacity: 0.8,
            speed: 0.0,
            rpm: 0.0,
            throttle: 0.0,
            brake: 0.0,
            steering: 0.0,
            lateral_g: 0.0,
            longitudinal_g: 0.0,
            speed_history: Vec::new(),
            rpm_history: Vec::new(),
            throttle_history: Vec::new(),
            brake_history: Vec::new(),
            steering_history: Vec::new(),
            lateral_g_history: Vec::new(),
            longitudinal_g_history: Vec::new(),
        }
    }
}

impl TelemetryOverlay {
    /// Capacità degli storico (port di `HISTORY_SIZE`).
    pub const HISTORY_SIZE: usize = 200;

    pub fn set_visible(&mut self, v: bool) {
        self.visible = v;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn toggle_visible(&mut self) {
        self.visible = !self.visible;
    }

    pub fn set_opacity(&mut self, o: f32) {
        self.opacity = o;
    }

    pub fn update(&mut self, d: TelemetryData) {
        self.speed = d.speed;
        self.rpm = d.rpm;
        self.throttle = d.throttle;
        self.brake = d.brake;
        self.steering = d.steering;
        self.lateral_g = d.lateral_g;
        self.longitudinal_g = d.longitudinal_g;
        push_cap(&mut self.speed_history, d.speed);
        push_cap(&mut self.rpm_history, d.rpm);
        push_cap(&mut self.throttle_history, d.throttle);
        push_cap(&mut self.brake_history, d.brake);
        push_cap(&mut self.steering_history, d.steering);
        push_cap(&mut self.lateral_g_history, d.lateral_g);
        push_cap(&mut self.longitudinal_g_history, d.longitudinal_g);
    }
}

/// Dati per `TelemetryOverlay::update`.
#[derive(Clone, Copy, Debug, Default)]
pub struct TelemetryData {
    pub speed: f32,
    pub rpm: f32,
    pub throttle: f32,
    pub brake: f32,
    pub steering: f32,
    pub lateral_g: f32,
    pub longitudinal_g: f32,
}

fn push_cap(v: &mut Vec<f32>, value: f32) {
    if v.len() >= TelemetryOverlay::HISTORY_SIZE {
        v.remove(0);
    }
    v.push(value);
}

// =============================================================================
// BEVY
// =============================================================================

/// Risorse HUD: il modello + la riga di stato decisa dal loop.
#[derive(Resource, Default)]
pub struct HudState {
    pub hud: RaceTelemetryHud,
    pub status: String,
}

/// Danni per il HUD, forniti dal loop (B3).
#[derive(Resource, Clone, Copy, Debug)]
pub struct HudDamage {
    pub damage_overall: f32,
    pub engine_health: f32,
    pub power_mult: f32,
    pub damage_warning: i32,
    pub engine_seized: bool,
}

impl Default for HudDamage {
    fn default() -> Self {
        Self {
            damage_overall: 0.0,
            engine_health: 1.0,
            power_mult: 1.0,
            damage_warning: 0,
            engine_seized: false,
        }
    }
}

/// Componente radice del HUD.
#[derive(Component)]
pub struct RaceHudRoot;

/// Cicla la modalità del HUD con F2.
pub fn cycle_hud_mode(keys: Res<ButtonInput<KeyCode>>, mut state: ResMut<HudState>) {
    if keys.just_pressed(KeyCode::F2) {
        state.hud.cycle_mode();
    }
}

/// Costruisce un campione dai dati disponibili e lo spinge nel HUD.
pub fn sync_hud(
    tel: Option<Res<CockpitTelemetry>>,
    dmg: Res<HudDamage>,
    mut state: ResMut<HudState>,
) {
    let status = state.status.clone();
    let sample = match tel {
        Some(tel) => RaceHudSample {
            speed_ms: tel.speed_ms,
            rpm: tel.rpm,
            max_rpm: if tel.max_rpm > 0.0 {
                tel.max_rpm
            } else {
                8500.0
            },
            gear: tel.gear as i32,
            throttle: tel.throttle,
            brake: tel.brake,
            position: tel.position as i32,
            total_cars: 1,
            lap: tel.current_lap as i32,
            total_laps: tel.total_laps as i32,
            current_time_ms: (tel.lap_time * 1000.0) as i32,
            best_time_ms: (tel.best_lap * 1000.0) as i32,
            fuel_l: tel.fuel_percent,
            damage_overall: dmg.damage_overall,
            engine_health: dmg.engine_health,
            power_mult: dmg.power_mult,
            damage_warning: dmg.damage_warning,
            engine_seized: dmg.engine_seized,
            status_text: status,
            ..Default::default()
        },
        None => return,
    };
    state.hud.push(sample);
}

/// Spawna la radice e le righe del HUD di gara.
pub fn spawn_race_hud(commands: &mut Commands) {
    let rows: [(HudElement, f32, f32, f32, Color); 8] = [
        (HudElement::Position, 24.0, 16.0, 26.0, Color::WHITE),
        (
            HudElement::Lap,
            220.0,
            16.0,
            22.0,
            Color::srgb(0.9, 0.9, 0.92),
        ),
        (
            HudElement::Status,
            420.0,
            16.0,
            22.0,
            Color::srgb(1.0, 0.85, 0.3),
        ),
        (
            HudElement::GForce,
            24.0,
            52.0,
            18.0,
            Color::srgb(0.8, 0.8, 0.85),
        ),
        (
            HudElement::Damage,
            24.0,
            76.0,
            18.0,
            Color::srgb(0.95, 0.3, 0.3),
        ),
        (HudElement::Speed, 560.0, 640.0, 48.0, Color::WHITE),
        (
            HudElement::Gear,
            700.0,
            660.0,
            40.0,
            Color::srgb(0.95, 0.2, 0.2),
        ),
        (
            HudElement::Rpm,
            560.0,
            700.0,
            20.0,
            Color::srgb(0.85, 0.85, 0.9),
        ),
    ];

    commands
        .spawn((
            RaceHudRoot,
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
        ))
        .with_children(|root| {
            for (el, left, top, size, color) in rows {
                root.spawn((
                    Text::new(""),
                    TextFont {
                        font_size: FontSize::Px(size),
                        ..default()
                    },
                    TextColor(color),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(left),
                        top: Val::Px(top),
                        ..default()
                    },
                    el,
                ));
            }
        });
}

/// Applica il modello ai nodi di testo (testo + visibilità per modalità).
pub fn update_race_hud(
    state: Res<HudState>,
    mut q: Query<(&HudElement, &mut Text, &mut Visibility)>,
) {
    if !state.hud.is_visible() {
        for (_, _, mut vis) in &mut q {
            *vis = Visibility::Hidden;
        }
        return;
    }
    for (el, mut text, mut vis) in &mut q {
        match state.hud.line(*el) {
            Some(s) => {
                **text = s;
                *vis = Visibility::Visible;
            }
            None => *vis = Visibility::Hidden,
        }
    }
}

/// Plugin Bevy: modello HUD + ciclaggio modalità + aggiornamento nodi.
pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudState>()
            .init_resource::<HudDamage>()
            .add_systems(Update, (cycle_hud_mode, sync_hud, update_race_hud).chain());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_cycles_through_all() {
        let m = RaceHudMode::Off;
        assert_eq!(m.next(), RaceHudMode::Compact);
        assert_eq!(RaceHudMode::Compact.next(), RaceHudMode::Race);
        assert_eq!(RaceHudMode::Race.next(), RaceHudMode::Engineer);
        assert_eq!(RaceHudMode::Engineer.next(), RaceHudMode::Off);
        assert!(!RaceHudMode::Off.is_visible());
        assert!(RaceHudMode::Race.is_visible());
    }

    #[test]
    fn default_mode_is_race_and_visible() {
        let hud = RaceTelemetryHud::default();
        assert_eq!(hud.mode(), RaceHudMode::Race);
        assert!(hud.is_visible());
    }

    #[test]
    fn cycle_mode_updates_field() {
        let mut hud = RaceTelemetryHud::default();
        hud.cycle_mode();
        assert_eq!(hud.mode(), RaceHudMode::Engineer);
    }

    #[test]
    fn format_time_ms_handles_sentinels() {
        assert_eq!(format_time_ms(0), "--:--.---");
        assert_eq!(format_time_ms(-5), "--:--.---");
        assert_eq!(format_time_ms(83_250), "1:23.250");
    }

    #[test]
    fn dashboard_format_time_handles_sentinels() {
        assert_eq!(DashboardOverlay::format_time(1e9), "--:--.---");
        assert_eq!(DashboardOverlay::format_time(-1.0), "--:--.---");
        assert_eq!(DashboardOverlay::format_time(f32::NAN), "--:--.---");
        assert_eq!(DashboardOverlay::format_time(83.25), "1:23.250");
    }

    #[test]
    fn push_records_sample_and_caps_history() {
        let mut hud = RaceTelemetryHud::default();
        let sample = RaceHudSample {
            speed_ms: 50.0,
            rpm: 4250.0,
            max_rpm: 8500.0,
            throttle: 0.5,
            brake: 0.1,
            ..Default::default()
        };
        hud.push(sample);
        assert_eq!(hud.sample().speed_ms, 50.0);
        assert_eq!(hud.speed_history().len(), 1);
        assert!((hud.speed_history()[0] - 180.0).abs() < 1e-3, "km/h");
        assert!((hud.rpm_history()[0] - 0.5).abs() < 1e-6, "normalized");

        for _ in 0..(RaceTelemetryHud::HIST_SIZE + 50) {
            hud.push(RaceHudSample::default());
        }
        assert_eq!(hud.speed_history().len(), RaceTelemetryHud::HIST_SIZE);
    }

    #[test]
    fn delta_ms_needs_both_times() {
        let mut hud = RaceTelemetryHud::default();
        hud.push(RaceHudSample {
            last_time_ms: 90_000,
            best_time_ms: 89_000,
            ..Default::default()
        });
        assert_eq!(hud.delta_ms(), 1000);

        let mut hud = RaceTelemetryHud::default();
        hud.push(RaceHudSample {
            last_time_ms: 90_000,
            ..Default::default()
        });
        assert_eq!(hud.delta_ms(), 0);
    }

    #[test]
    fn compact_mode_shows_only_compact_lines() {
        let mut hud = RaceTelemetryHud::default();
        hud.set_mode(RaceHudMode::Compact);
        hud.push(RaceHudSample {
            speed_ms: 33.3,
            position: 4,
            ..Default::default()
        });
        assert_eq!(hud.line(HudElement::Position).unwrap(), "P4");
        assert_eq!(hud.line(HudElement::Speed).unwrap(), "120");
        assert!(hud.line(HudElement::Gear).is_none());
        assert!(hud.line(HudElement::Lap).is_none());
        // Damage hidden in Compact until a warning is raised.
        assert!(hud.line(HudElement::Damage).is_none());
    }

    #[test]
    fn race_mode_shows_lap_gear_and_full_position() {
        let mut hud = RaceTelemetryHud::default();
        hud.push(RaceHudSample {
            position: 3,
            total_cars: 12,
            lap: 5,
            total_laps: 20,
            gear: 4,
            ..Default::default()
        });
        assert_eq!(hud.line(HudElement::Position).unwrap(), "P3/12");
        assert_eq!(hud.line(HudElement::Lap).unwrap(), "LAP 5/20");
        assert_eq!(hud.line(HudElement::Gear).unwrap(), "4");
        assert!(hud.line(HudElement::GForce).is_none());
    }

    #[test]
    fn engineer_mode_adds_force_and_damage_lines() {
        let mut hud = RaceTelemetryHud::default();
        hud.set_mode(RaceHudMode::Engineer);
        hud.push(RaceHudSample {
            lat_g: 1.234,
            lon_g: -0.5,
            damage_overall: 0.42,
            engine_health: 0.8,
            power_mult: 0.9,
            engine_seized: true,
            ..Default::default()
        });
        assert_eq!(
            hud.line(HudElement::GForce).unwrap(),
            "G lat +1.23  lon -0.50"
        );
        let dmg = hud.line(HudElement::Damage).unwrap();
        assert!(dmg.contains("DMG 42%"), "{dmg}");
        assert!(dmg.contains("SEIZED"), "{dmg}");
    }

    #[test]
    fn off_mode_hides_everything() {
        let mut hud = RaceTelemetryHud::default();
        hud.set_mode(RaceHudMode::Off);
        for el in [
            HudElement::Position,
            HudElement::Lap,
            HudElement::Status,
            HudElement::Speed,
            HudElement::Gear,
            HudElement::Rpm,
            HudElement::GForce,
            HudElement::Damage,
        ] {
            assert!(hud.line(el).is_none(), "{el:?} should be hidden");
        }
    }

    #[test]
    fn empty_status_line_is_hidden() {
        let mut hud = RaceTelemetryHud::default();
        assert!(hud.line(HudElement::Status).is_none());
        hud.set_status("REFUEL 60 L");
        assert_eq!(hud.line(HudElement::Status).unwrap(), "REFUEL 60 L");
    }

    #[test]
    fn dashboard_overlay_updates_and_formats() {
        let mut dash = DashboardOverlay::default();
        assert!(dash.is_visible());
        dash.set_opacity(0.5);
        assert_eq!(dash.opacity, 0.5);
        dash.update(DashboardData {
            speed: 60.0,
            rpm: 7000.0,
            gear: 3,
            current_lap: 2,
            lap_time: 95.5,
            best_lap_time: 94.0,
            last_lap_time: 95.5,
            position: 2,
            total_cars: 8,
            ..Default::default()
        });
        assert_eq!(dash.gear, 3);
        assert_eq!(dash.total_cars, 8);
        assert_eq!(DashboardOverlay::format_time(dash.lap_time), "1:35.500");
    }

    #[test]
    fn telemetry_overlay_toggle_and_history_cap() {
        let mut ov = TelemetryOverlay::default();
        assert!(!ov.is_visible());
        ov.toggle_visible();
        assert!(ov.is_visible());

        for i in 0..(TelemetryOverlay::HISTORY_SIZE + 25) {
            ov.update(TelemetryData {
                speed: i as f32,
                ..Default::default()
            });
        }
        assert_eq!(ov.speed_history.len(), TelemetryOverlay::HISTORY_SIZE);
        assert_eq!(
            *ov.speed_history.last().unwrap(),
            (TelemetryOverlay::HISTORY_SIZE + 24) as f32
        );
    }

    #[test]
    fn plugin_builds_without_panicking() {
        let mut app = App::new();
        app.init_resource::<ButtonInput<KeyCode>>();
        app.add_plugins(HudPlugin);
        app.update();
    }
}
