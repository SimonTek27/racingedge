//! Cockpit instruments — classic dashboard style
//!
//! **Digital instruments** (`digital_instruments.ini` → ITEM_n):
//!   SPEED, RPM, GEAR, LAPTIME, BEST_LAP, FUEL, TC_LEVEL, ABS_LEVEL, …
//!
//! **Analog instruments** (`analog_instruments.ini`):
//!   aghi RPM / SPEED / TURBO / TEMP con INPUT_MIN / INPUT_MAX → angolo.

use bevy::prelude::*;

use crate::simulator::physics::VehicleState;

// =============================================================================
// DIGITAL INSTRUMENTS
// =============================================================================

/// Tipo di valore mostrato (come TYPE in digital_instruments.ini).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DigitalType {
    Gear,
    Speed,
    Rpm,
    MaxRpm,
    Fuel,
    LapTime,
    BestLap,
    CurrentLap,
    TotalLaps,
    WaterTemp,
    TurboBoost,
    Position,
    Delta,
    ErsCharge,
}

/// Formato di visualizzazione (DIGITAL_ITEM_NUMBER_FORMAT).
#[derive(Clone, Debug)]
pub enum DigitalFormat {
    /// Intero a larghezza fissa, es. 3 cifre velocità
    Integer { width: usize },
    /// Decimale, es. 1 cifra dopo la virgola
    Decimal { width: usize, frac: usize },
    /// Solo marcia R/N/1..
    Gear,
    /// Tempo mm:ss.mmm
    LapTime,
    /// Testo libero (placeholder)
    Text,
}

impl DigitalFormat {
    pub fn format_value(&self, value: f32, gear: i8) -> String {
        match self {
            DigitalFormat::Integer { width } => {
                format!("{:0width$}", value.round() as i32, width = *width)
            }
            DigitalFormat::Decimal { width, frac } => {
                format!(
                    "{:width$.frac$}",
                    value,
                    width = width + frac + 1,
                    frac = *frac
                )
            }
            DigitalFormat::Gear => match gear {
                -1 => "R".into(),
                0 => "N".into(),
                g => g.to_string(),
            },
            DigitalFormat::LapTime => {
                if value >= f32::MAX / 2.0 || value < 0.0 {
                    "--:--.---".into()
                } else {
                    let m = (value / 60.0).floor() as u32;
                    let s = value % 60.0;
                    format!("{:02}:{:06.3}", m, s)
                }
            }
            DigitalFormat::Text => String::new(),
        }
    }
}

/// Un display digitale (ITEM_n).
#[derive(Component, Clone, Debug)]
pub struct DigitalInstrument {
    pub kind: DigitalType,
    pub format: DigitalFormat,
    /// Moltiplicatore (es. 3.6 per m/s → km/h)
    pub mult: f32,
    pub upper_bound: Option<f32>,
}

impl DigitalInstrument {
    pub fn speed_kmh() -> Self {
        Self {
            kind: DigitalType::Speed,
            format: DigitalFormat::Integer { width: 3 },
            mult: 3.6,
            upper_bound: Some(400.0),
        }
    }

    pub fn rpm() -> Self {
        Self {
            kind: DigitalType::Rpm,
            format: DigitalFormat::Integer { width: 5 },
            mult: 1.0,
            upper_bound: None,
        }
    }

    pub fn gear() -> Self {
        Self {
            kind: DigitalType::Gear,
            format: DigitalFormat::Gear,
            mult: 1.0,
            upper_bound: None,
        }
    }

    pub fn lap_time() -> Self {
        Self {
            kind: DigitalType::LapTime,
            format: DigitalFormat::LapTime,
            mult: 1.0,
            upper_bound: None,
        }
    }

    pub fn best_lap() -> Self {
        Self {
            kind: DigitalType::BestLap,
            format: DigitalFormat::LapTime,
            mult: 1.0,
            upper_bound: None,
        }
    }

    pub fn fuel_percent() -> Self {
        Self {
            kind: DigitalType::Fuel,
            format: DigitalFormat::Integer { width: 3 },
            mult: 1.0,
            upper_bound: Some(100.0),
        }
    }
}

/// Snapshot telemetria per gli strumenti (riempito ogni frame).
#[derive(Resource, Clone, Debug, Default)]
pub struct CockpitTelemetry {
    pub speed_ms: f32,
    pub rpm: f32,
    pub max_rpm: f32,
    pub gear: i8,
    pub throttle: f32,
    pub brake: f32,
    pub fuel_percent: f32,
    pub water_temp_c: f32,
    pub turbo_boost: f32,
    pub lap_time: f32,
    pub best_lap: f32,
    pub current_lap: u32,
    pub total_laps: u32,
    pub position: u32,
    pub ers_charge: f32,
    pub delta: f32,
}

// =============================================================================
// ANALOG INSTRUMENTS
// =============================================================================

/// Ingresso analogico (come INPUT in analog_instruments / CSP).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnalogInput {
    Rpm,
    Speed,
    TurboBoost,
    WaterTemp,
    Fuel,
    Throttle,
    Brake,
    ErsCharge,
}

/// Analog needle: maps a value → angle [rad] like the classic dashboard
/// (OBJECT_NAME + MIN/MAX + end-of-travel angles).
#[derive(Component, Clone, Debug)]
pub struct AnalogInstrument {
    pub input: AnalogInput,
    /// Valore a fine corsa sinistra / bassa
    pub input_min: f32,
    /// Valore a fine corsa destra / alta
    pub input_max: f32,
    /// Angolo [rad] a input_min
    pub angle_min: f32,
    /// Angolo [rad] a input_max
    pub angle_max: f32,
    /// Landa (smoothing 0..1, 1 = istantaneo)
    pub lag: f32,
    /// Angolo corrente interpolato
    pub current_angle: f32,
}

impl AnalogInstrument {
    /// Contagiri tipico: 0–8000 rpm, arco ~240°.
    pub fn rpm_gauge(max_rpm: f32) -> Self {
        Self {
            input: AnalogInput::Rpm,
            input_min: 0.0,
            input_max: max_rpm.max(1000.0),
            angle_min: -2.1, // ~-120°
            angle_max: 2.1,  // ~+120°
            lag: 0.25,
            current_angle: -2.1,
        }
    }

    /// Tachimetro 0–300 km/h.
    pub fn speed_gauge(max_kmh: f32) -> Self {
        Self {
            input: AnalogInput::Speed,
            input_min: 0.0,
            input_max: max_kmh.max(50.0),
            angle_min: -2.0,
            angle_max: 2.0,
            lag: 0.2,
            current_angle: -2.0,
        }
    }

    pub fn fuel_gauge() -> Self {
        Self {
            input: AnalogInput::Fuel,
            input_min: 0.0,
            input_max: 100.0,
            angle_min: -1.2,
            angle_max: 1.2,
            lag: 0.05,
            current_angle: 1.2,
        }
    }

    fn raw_value(&self, tel: &CockpitTelemetry) -> f32 {
        match self.input {
            AnalogInput::Rpm => tel.rpm,
            AnalogInput::Speed => tel.speed_ms * 3.6,
            AnalogInput::TurboBoost => tel.turbo_boost,
            AnalogInput::WaterTemp => tel.water_temp_c,
            AnalogInput::Fuel => tel.fuel_percent,
            AnalogInput::Throttle => tel.throttle * 100.0,
            AnalogInput::Brake => tel.brake * 100.0,
            AnalogInput::ErsCharge => tel.ers_charge * 100.0,
        }
    }

    pub fn target_angle(&self, tel: &CockpitTelemetry) -> f32 {
        let v = self.raw_value(tel).clamp(self.input_min, self.input_max);
        let t = if (self.input_max - self.input_min).abs() < 1e-6 {
            0.0
        } else {
            (v - self.input_min) / (self.input_max - self.input_min)
        };
        self.angle_min + t * (self.angle_max - self.angle_min)
    }
}

/// Mesh ago (figlio dell’entità AnalogInstrument) — ruota su Z locale.
#[derive(Component)]
pub struct AnalogNeedle;

// =============================================================================
// SYSTEMS
// =============================================================================

/// Copia stato veicolo → telemetria cockpit.
pub fn sync_telemetry(mut tel: ResMut<CockpitTelemetry>, q: Query<&VehicleState>) {
    let Ok(car) = q.single() else {
        return;
    };
    tel.speed_ms = car.speed.abs();
    tel.rpm = car.engine.rpm;
    tel.gear = car.drivetrain.gear;
    tel.throttle = car.throttle;
    tel.brake = car.brake;
    // placeholder finché non c’è fuel model completo
    if tel.fuel_percent <= 0.0 {
        tel.fuel_percent = 80.0;
    }
    if tel.water_temp_c <= 0.0 {
        tel.water_temp_c = 90.0;
    }
    if tel.max_rpm <= 0.0 {
        tel.max_rpm = 8000.0;
    }
}

/// Aggiorna testo dei display digitali.
pub fn update_digital(tel: Res<CockpitTelemetry>, mut q: Query<(&DigitalInstrument, &mut Text)>) {
    for (inst, mut text) in &mut q {
        let raw = match inst.kind {
            DigitalType::Gear => 0.0,
            DigitalType::Speed => tel.speed_ms,
            DigitalType::Rpm => tel.rpm,
            DigitalType::MaxRpm => tel.max_rpm,
            DigitalType::Fuel => tel.fuel_percent,
            DigitalType::LapTime => tel.lap_time,
            DigitalType::BestLap => tel.best_lap,
            DigitalType::CurrentLap => tel.current_lap as f32,
            DigitalType::TotalLaps => tel.total_laps as f32,
            DigitalType::WaterTemp => tel.water_temp_c,
            DigitalType::TurboBoost => tel.turbo_boost,
            DigitalType::Position => tel.position as f32,
            DigitalType::Delta => tel.delta,
            DigitalType::ErsCharge => tel.ers_charge * 100.0,
        };
        let mut v = raw * inst.mult;
        if let Some(ub) = inst.upper_bound {
            v = v.min(ub);
        }
        **text = inst.format.format_value(v, tel.gear);
    }
}

/// Aggiorna angolo aghi analogici.
pub fn update_analog(
    time: Res<Time>,
    tel: Res<CockpitTelemetry>,
    mut q: Query<(&mut AnalogInstrument, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (mut inst, mut tf) in &mut q {
        let target = inst.target_angle(&tel);
        let alpha = (inst.lag * 60.0 * dt).clamp(0.0, 1.0);
        inst.current_angle += (target - inst.current_angle) * alpha;
        // Rotazione ago sul piano del cruscotto (asse Z locale)
        tf.rotation = Quat::from_rotation_z(inst.current_angle);
    }
}

/// Plugin Bevy: telemetria + digital + analog.
pub struct CockpitPlugin;

impl Plugin for CockpitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CockpitTelemetry>().add_systems(
            Update,
            (sync_telemetry, update_digital, update_analog).chain(),
        );
    }
}

/// Spawna un set base di strumenti digitali HUD (stile race).
pub fn spawn_default_digital_hud(commands: &mut Commands) {
    let style_base = |top: f32, left: f32| Node {
        position_type: PositionType::Absolute,
        top: Val::Px(top),
        left: Val::Px(left),
        ..default()
    };

    // Gear — grande al centro basso
    commands.spawn((
        Text::new("1"),
        style_base(620.0, 600.0),
        TextFont {
            font_size: FontSize::Px(64.0),
            ..default()
        },
        TextColor(Color::srgb(1.0, 0.9, 0.2)),
        DigitalInstrument::gear(),
    ));

    // Speed
    commands.spawn((
        Text::new("000"),
        style_base(640.0, 520.0),
        TextFont {
            font_size: FontSize::Px(36.0),
            ..default()
        },
        TextColor(Color::WHITE),
        DigitalInstrument::speed_kmh(),
    ));

    // RPM
    commands.spawn((
        Text::new("00000"),
        style_base(640.0, 700.0),
        TextFont {
            font_size: FontSize::Px(28.0),
            ..default()
        },
        TextColor(Color::srgb(0.9, 0.9, 0.9)),
        DigitalInstrument::rpm(),
    ));

    // Lap time
    commands.spawn((
        Text::new("--:--.---"),
        style_base(12.0, 1000.0),
        TextFont {
            font_size: FontSize::Px(22.0),
            ..default()
        },
        TextColor(Color::srgb(0.7, 1.0, 0.7)),
        DigitalInstrument::lap_time(),
    ));

    // Best lap
    commands.spawn((
        Text::new("--:--.---"),
        style_base(40.0, 1000.0),
        TextFont {
            font_size: FontSize::Px(18.0),
            ..default()
        },
        TextColor(Color::srgb(0.5, 0.8, 1.0)),
        DigitalInstrument::best_lap(),
    ));
}

/// Spawna un ago analogico RPM 3D semplice (cilindro sottile come ago).
pub fn spawn_analog_rpm_needle(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    origin: Vec3,
    max_rpm: f32,
) {
    let needle_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.95, 0.15, 0.1),
        unlit: true,
        ..default()
    });
    let face_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.08, 0.08, 0.1),
        ..default()
    });

    // Quadrante
    commands.spawn((
        Mesh3d(meshes.add(Cylinder::new(0.55, 0.04))),
        MeshMaterial3d(face_mat),
        Transform::from_translation(origin)
            .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
    ));

    // Ago
    commands
        .spawn((
            Transform::from_translation(origin + Vec3::Y * 0.05),
            AnalogInstrument::rpm_gauge(max_rpm),
            AnalogNeedle,
            Visibility::default(),
        ))
        .with_children(|p| {
            p.spawn((
                Mesh3d(meshes.add(Cuboid::new(0.04, 0.02, 0.5))),
                MeshMaterial3d(needle_mat),
                Transform::from_xyz(0.0, 0.0, -0.2),
            ));
        });
}
