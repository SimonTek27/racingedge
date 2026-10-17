//! Setup dell'auto: modello dei parametri, schermata garage e persistenza.
//!
//! Port di `src/simulator/SetupGarage.{h,cpp}` e `src/simulator/ApplySetup.h`
//! di ksengine. Il disegno `ui::DrawList` resta all'applicazione; qui vivono il
//! modello, la navigazione a schede/righe e la persistenza per auto/pista.

use std::path::{Path, PathBuf};

/// Scheda del garage (port di `GarageTab`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum GarageTab {
    /// Panoramica.
    #[default]
    Overview,
    /// Pneumatici.
    Tyres,
    /// Sospensioni.
    Suspension,
    /// Aerodinamica.
    Aero,
    /// Freni.
    Brakes,
    /// Differenziale.
    Gears,
}

impl GarageTab {
    /// Numero di schede (port di `GarageTab::Count`).
    pub const COUNT: usize = 6;

    /// Tutte le schede in ordine.
    pub const ALL: [GarageTab; Self::COUNT] = [
        GarageTab::Overview,
        GarageTab::Tyres,
        GarageTab::Suspension,
        GarageTab::Aero,
        GarageTab::Brakes,
        GarageTab::Gears,
    ];

    /// Nome leggibile (port di `tabName`).
    pub fn name(self) -> &'static str {
        match self {
            GarageTab::Overview => "OVERVIEW",
            GarageTab::Tyres => "TYRES",
            GarageTab::Suspension => "SUSPENSION",
            GarageTab::Aero => "AERO",
            GarageTab::Brakes => "BRAKES",
            GarageTab::Gears => "DIFF",
        }
    }

    fn index(self) -> i32 {
        match self {
            GarageTab::Overview => 0,
            GarageTab::Tyres => 1,
            GarageTab::Suspension => 2,
            GarageTab::Aero => 3,
            GarageTab::Brakes => 4,
            GarageTab::Gears => 5,
        }
    }

    fn from_index(i: i32) -> Self {
        match i.rem_euclid(Self::COUNT as i32) {
            0 => GarageTab::Overview,
            1 => GarageTab::Tyres,
            2 => GarageTab::Suspension,
            3 => GarageTab::Aero,
            4 => GarageTab::Brakes,
            _ => GarageTab::Gears,
        }
    }
}

/// Dati del setup (port di `SetupData`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SetupData {
    /// Pressione anteriore sinistra (bar).
    pub tire_pressure_fl: f32,
    /// Pressione anteriore destra (bar).
    pub tire_pressure_fr: f32,
    /// Pressione posteriore sinistra (bar).
    pub tire_pressure_rl: f32,
    /// Pressione posteriore destra (bar).
    pub tire_pressure_rr: f32,
    /// Ripartizione frenata (frazione sull'anteriore, 0..1).
    pub brake_bias: f32,
    /// Altezza da terra anteriore (mm).
    pub ride_height_front: f32,
    /// Altezza da terra posteriore (mm).
    pub ride_height_rear: f32,
    /// Rigidità molla anteriore (N/mm).
    pub spring_rate_front: f32,
    /// Rigidità molla posteriore (N/mm).
    pub spring_rate_rear: f32,
    /// Angolo ala anteriore (gradi).
    pub front_wing_angle: f32,
    /// Angolo ala posteriore (gradi).
    pub rear_wing_angle: f32,
    /// Precario differenziale (N·m).
    pub diff_preload: f32,
    /// Carburante (L).
    pub fuel: f32,
    /// Zavorra (kg).
    pub ballast: f32,
    /// Livello controllo di trazione.
    pub tc_level: i32,
    /// Livello ABS.
    pub abs_level: i32,
}

impl Default for SetupData {
    fn default() -> Self {
        Self {
            tire_pressure_fl: 2.20,
            tire_pressure_fr: 2.20,
            tire_pressure_rl: 2.00,
            tire_pressure_rr: 2.00,
            brake_bias: 0.56,
            ride_height_front: 30.0,
            ride_height_rear: 35.0,
            spring_rate_front: 150.0,
            spring_rate_rear: 180.0,
            front_wing_angle: 10.0,
            rear_wing_angle: 12.0,
            diff_preload: 30.0,
            fuel: 50.0,
            ballast: 0.0,
            tc_level: 0,
            abs_level: 0,
        }
    }
}

/// Parametri applicabili alla vettura (port di `SetupParams`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SetupParams {
    /// Pressioni dei quattro pneumatici (FL, FR, RL, RR).
    pub tire_pressure: [f32; 4],
    /// Ripartizione frenata.
    pub brake_bias: f32,
    /// Ala anteriore.
    pub front_wing: f32,
    /// Ala posteriore.
    pub rear_wing: f32,
    /// Zavorra.
    pub ballast: f32,
    /// Carburante.
    pub fuel: f32,
    /// Campanatura (FL, FR, RL, RR).
    pub camber: [f32; 4],
    /// Convergenza (FL, FR, RL, RR).
    pub toe: [f32; 4],
}

impl Default for SetupParams {
    fn default() -> Self {
        Self {
            tire_pressure: [1.8, 1.8, 1.8, 1.8],
            brake_bias: 0.55,
            front_wing: 5.0,
            rear_wing: 8.0,
            ballast: 0.0,
            fuel: 40.0,
            camber: [-2.5, -2.5, -1.5, -1.5],
            toe: [0.0, 0.0, 0.1, 0.1],
        }
    }
}

impl SetupParams {
    /// Deriva i parametri vettura da un setup garage.
    pub fn from_setup_data(s: &SetupData) -> Self {
        Self {
            tire_pressure: [
                s.tire_pressure_fl,
                s.tire_pressure_fr,
                s.tire_pressure_rl,
                s.tire_pressure_rr,
            ],
            brake_bias: s.brake_bias,
            front_wing: s.front_wing_angle,
            rear_wing: s.rear_wing_angle,
            ballast: s.ballast,
            fuel: s.fuel,
            ..Self::default()
        }
    }
}

/// Campo modificabile di una riga del garage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupField {
    /// Carburante.
    Fuel,
    /// Zavorra.
    Ballast,
    /// Controllo di trazione.
    TcLevel,
    /// ABS.
    AbsLevel,
    /// Pressione FL.
    PressureFl,
    /// Pressione FR.
    PressureFr,
    /// Pressione RL.
    PressureRl,
    /// Pressione RR.
    PressureRr,
    /// Altezza anteriore.
    RideHeightFront,
    /// Altezza posteriore.
    RideHeightRear,
    /// Molla anteriore.
    SpringRateFront,
    /// Molla posteriore.
    SpringRateRear,
    /// Ala anteriore.
    FrontWingAngle,
    /// Ala posteriore.
    RearWingAngle,
    /// Ripartizione frenata.
    BrakeBias,
    /// Precario differenziale.
    DiffPreload,
}

impl SetupField {
    fn get(self, data: &SetupData) -> f32 {
        match self {
            SetupField::Fuel => data.fuel,
            SetupField::Ballast => data.ballast,
            SetupField::TcLevel => data.tc_level as f32,
            SetupField::AbsLevel => data.abs_level as f32,
            SetupField::PressureFl => data.tire_pressure_fl,
            SetupField::PressureFr => data.tire_pressure_fr,
            SetupField::PressureRl => data.tire_pressure_rl,
            SetupField::PressureRr => data.tire_pressure_rr,
            SetupField::RideHeightFront => data.ride_height_front,
            SetupField::RideHeightRear => data.ride_height_rear,
            SetupField::SpringRateFront => data.spring_rate_front,
            SetupField::SpringRateRear => data.spring_rate_rear,
            SetupField::FrontWingAngle => data.front_wing_angle,
            SetupField::RearWingAngle => data.rear_wing_angle,
            SetupField::BrakeBias => data.brake_bias,
            SetupField::DiffPreload => data.diff_preload,
        }
    }

    fn set(self, data: &mut SetupData, value: f32) {
        match self {
            SetupField::Fuel => data.fuel = value,
            SetupField::Ballast => data.ballast = value,
            SetupField::TcLevel => data.tc_level = value as i32,
            SetupField::AbsLevel => data.abs_level = value as i32,
            SetupField::PressureFl => data.tire_pressure_fl = value,
            SetupField::PressureFr => data.tire_pressure_fr = value,
            SetupField::PressureRl => data.tire_pressure_rl = value,
            SetupField::PressureRr => data.tire_pressure_rr = value,
            SetupField::RideHeightFront => data.ride_height_front = value,
            SetupField::RideHeightRear => data.ride_height_rear = value,
            SetupField::SpringRateFront => data.spring_rate_front = value,
            SetupField::SpringRateRear => data.spring_rate_rear = value,
            SetupField::FrontWingAngle => data.front_wing_angle = value,
            SetupField::RearWingAngle => data.rear_wing_angle = value,
            SetupField::BrakeBias => data.brake_bias = value,
            SetupField::DiffPreload => data.diff_preload = value,
        }
    }
}

/// Riga della lista del garage (port di `GarageRow`, con il campo tipizzato
/// al posto dei puntatori).
#[derive(Clone, Copy, Debug)]
pub struct GarageRow {
    /// Etichetta.
    pub label: &'static str,
    /// Unità di misura.
    pub unit: &'static str,
    /// Descrizione.
    pub hint: &'static str,
    /// Campo modificato.
    pub field: SetupField,
    /// Minimo (float).
    pub min_v: f32,
    /// Massimo (float).
    pub max_v: f32,
    /// Passo.
    pub step: f32,
    /// È un intero.
    pub is_int: bool,
    /// Minimo (int).
    pub int_min: i32,
    /// Massimo (int).
    pub int_max: i32,
}

impl GarageRow {
    fn float(
        label: &'static str,
        unit: &'static str,
        hint: &'static str,
        field: SetupField,
        min_v: f32,
        max_v: f32,
        step: f32,
    ) -> Self {
        Self {
            label,
            unit,
            hint,
            field,
            min_v,
            max_v,
            step,
            is_int: false,
            int_min: 0,
            int_max: 0,
        }
    }

    fn integer(
        label: &'static str,
        hint: &'static str,
        field: SetupField,
        int_min: i32,
        int_max: i32,
    ) -> Self {
        Self {
            label,
            unit: "",
            hint,
            field,
            min_v: 0.0,
            max_v: 0.0,
            step: 0.0,
            is_int: true,
            int_min,
            int_max,
        }
    }

    /// Valore normalizzato 0..1 per le barre della UI.
    pub fn normalized(&self, data: &SetupData) -> f32 {
        if self.is_int {
            if self.int_max > self.int_min {
                (self.field.get(data) - self.int_min as f32) / (self.int_max - self.int_min) as f32
            } else {
                0.0
            }
        } else if self.max_v > self.min_v {
            (self.field.get(data) - self.min_v) / (self.max_v - self.min_v)
        } else {
            0.0
        }
    }
}

/// Tasti usati dalla schermata (port dei codici di `handleKeyPress`).
pub const KEY_UP: i32 = 0x26;
/// Freccia giù.
pub const KEY_DOWN: i32 = 0x28;
/// Freccia sinistra.
pub const KEY_LEFT: i32 = 0x25;
/// Freccia destra.
pub const KEY_RIGHT: i32 = 0x27;
/// Tab.
pub const KEY_TAB: i32 = 0x09;
/// Esc.
pub const KEY_ESC: i32 = 0x1B;

/// Schermata garage / setup (port di `SetupGarage`).
pub struct SetupGarage {
    visible: bool,
    fade_in: f32,
    slide: f32,
    tab: GarageTab,
    selected_row: i32,
    setup: SetupData,
    rows: Vec<GarageRow>,
    car_name: String,
    track_name: String,
    on_closed: Option<Box<dyn FnMut()>>,
    on_setup_changed: Option<Box<dyn FnMut(SetupData)>>,
}

impl Default for SetupGarage {
    fn default() -> Self {
        Self::new()
    }
}

impl SetupGarage {
    /// Una schermata garage con setup predefinito.
    pub fn new() -> Self {
        let mut garage = Self {
            visible: false,
            fade_in: 0.0,
            slide: 0.0,
            tab: GarageTab::Overview,
            selected_row: 0,
            setup: SetupData::default(),
            rows: Vec::new(),
            car_name: String::new(),
            track_name: String::new(),
            on_closed: None,
            on_setup_changed: None,
        };
        garage.rebuild_rows();
        garage
    }

    /// Registra la callback di chiusura.
    pub fn set_on_closed(&mut self, callback: impl FnMut() + 'static) {
        self.on_closed = Some(Box::new(callback));
    }

    /// Registra la callback di modifica setup.
    pub fn set_on_setup_changed(&mut self, callback: impl FnMut(SetupData) + 'static) {
        self.on_setup_changed = Some(Box::new(callback));
    }

    /// Imposta la visibilità (port di `setVisible`).
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
        if visible {
            self.selected_row = 0;
            self.rebuild_rows();
        } else if let Some(cb) = self.on_closed.as_mut() {
            cb();
        }
    }

    /// La schermata è visibile.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Inverte la visibilità.
    pub fn toggle_visible(&mut self) {
        self.set_visible(!self.visible);
    }

    /// Opacità della dissolvenza.
    pub fn fade_alpha(&self) -> f32 {
        self.fade_in
    }

    /// Scostamento della dissolvenza (px).
    pub fn slide(&self) -> f32 {
        self.slide
    }

    /// Imposta il nome dell'auto.
    pub fn set_car_name(&mut self, name: impl Into<String>) {
        self.car_name = name.into();
    }

    /// Imposta il nome della pista.
    pub fn set_track_name(&mut self, name: impl Into<String>) {
        self.track_name = name.into();
    }

    /// Il nome dell'auto.
    pub fn car_name(&self) -> &str {
        &self.car_name
    }

    /// Il nome della pista.
    pub fn track_name(&self) -> &str {
        &self.track_name
    }

    /// Il setup corrente.
    pub fn current_setup(&self) -> &SetupData {
        &self.setup
    }

    /// Il setup corrente (mutabile).
    pub fn setup_mut(&mut self) -> &mut SetupData {
        &mut self.setup
    }

    /// Sostituisce il setup.
    pub fn set_setup(&mut self, setup: SetupData) {
        self.setup = setup;
        self.rebuild_rows();
    }

    /// La scheda corrente.
    pub fn tab(&self) -> GarageTab {
        self.tab
    }

    /// La riga selezionata.
    pub fn selected_row(&self) -> i32 {
        self.selected_row
    }

    /// Le righe della scheda corrente.
    pub fn rows(&self) -> &[GarageRow] {
        &self.rows
    }

    /// Ricostruisce le righe della scheda (port di `rebuildRows`).
    pub fn rebuild_rows(&mut self) {
        self.rows.clear();
        match self.tab {
            GarageTab::Overview => {
                self.rows.push(GarageRow::float(
                    "FUEL",
                    "L",
                    "Session fuel load",
                    SetupField::Fuel,
                    5.0,
                    120.0,
                    1.0,
                ));
                self.rows.push(GarageRow::float(
                    "BALLAST",
                    "kg",
                    "Additional mass",
                    SetupField::Ballast,
                    0.0,
                    80.0,
                    1.0,
                ));
                self.rows.push(GarageRow::integer(
                    "TC",
                    "Traction control level",
                    SetupField::TcLevel,
                    0,
                    10,
                ));
                self.rows.push(GarageRow::integer(
                    "ABS",
                    "Anti-lock level",
                    SetupField::AbsLevel,
                    0,
                    10,
                ));
            }
            GarageTab::Tyres => {
                for (label, hint, field) in [
                    (
                        "PRESSURE FL",
                        "Front left cold pressure",
                        SetupField::PressureFl,
                    ),
                    (
                        "PRESSURE FR",
                        "Front right cold pressure",
                        SetupField::PressureFr,
                    ),
                    (
                        "PRESSURE RL",
                        "Rear left cold pressure",
                        SetupField::PressureRl,
                    ),
                    (
                        "PRESSURE RR",
                        "Rear right cold pressure",
                        SetupField::PressureRr,
                    ),
                ] {
                    self.rows
                        .push(GarageRow::float(label, "bar", hint, field, 1.4, 3.2, 0.05));
                }
            }
            GarageTab::Suspension => {
                self.rows.push(GarageRow::float(
                    "RIDE HEIGHT F",
                    "mm",
                    "Front static ride height",
                    SetupField::RideHeightFront,
                    15.0,
                    80.0,
                    0.5,
                ));
                self.rows.push(GarageRow::float(
                    "RIDE HEIGHT R",
                    "mm",
                    "Rear static ride height",
                    SetupField::RideHeightRear,
                    15.0,
                    80.0,
                    0.5,
                ));
                self.rows.push(GarageRow::float(
                    "SPRING F",
                    "N/mm",
                    "Front spring rate",
                    SetupField::SpringRateFront,
                    60.0,
                    300.0,
                    2.0,
                ));
                self.rows.push(GarageRow::float(
                    "SPRING R",
                    "N/mm",
                    "Rear spring rate",
                    SetupField::SpringRateRear,
                    60.0,
                    300.0,
                    2.0,
                ));
            }
            GarageTab::Aero => {
                self.rows.push(GarageRow::float(
                    "FRONT WING",
                    "deg",
                    "Front wing angle",
                    SetupField::FrontWingAngle,
                    0.0,
                    30.0,
                    0.5,
                ));
                self.rows.push(GarageRow::float(
                    "REAR WING",
                    "deg",
                    "Rear wing angle",
                    SetupField::RearWingAngle,
                    0.0,
                    35.0,
                    0.5,
                ));
            }
            GarageTab::Brakes => {
                self.rows.push(GarageRow::float(
                    "BRAKE BIAS",
                    "%",
                    "Front brake bias",
                    SetupField::BrakeBias,
                    0.35,
                    0.75,
                    0.005,
                ));
            }
            GarageTab::Gears => {
                self.rows.push(GarageRow::float(
                    "DIFF PRELOAD",
                    "Nm",
                    "Differential preload",
                    SetupField::DiffPreload,
                    0.0,
                    120.0,
                    1.0,
                ));
            }
        }
        if self.selected_row >= self.rows.len() as i32 {
            self.selected_row = (self.rows.len() as i32 - 1).max(0);
        }
    }

    /// Passa alla scheda successiva/precedente (port di `nextTab`).
    pub fn next_tab(&mut self, delta: i32) {
        self.tab = GarageTab::from_index(self.tab.index() + delta);
        self.selected_row = 0;
        self.rebuild_rows();
    }

    /// Regola la riga selezionata (port di `adjustSelected`).
    pub fn adjust_selected(&mut self, dir: f32) {
        let Some(row) = self.rows.get(self.selected_row.max(0) as usize).copied() else {
            return;
        };
        if row.is_int {
            let current = row.field.get(&self.setup) as i32;
            let next = (current + if dir > 0.0 { 1 } else { -1 }).clamp(row.int_min, row.int_max);
            row.field.set(&mut self.setup, next as f32);
        } else {
            let next = (row.field.get(&self.setup) + dir * row.step).clamp(row.min_v, row.max_v);
            row.field.set(&mut self.setup, next);
        }
        if let Some(cb) = self.on_setup_changed.as_mut() {
            cb(self.setup);
        }
    }

    /// Avanza l'animazione di apertura (port di `update`).
    pub fn update(&mut self, dt: f32) {
        let target = if self.visible { 1.0 } else { 0.0 };
        let speed = 6.0;
        if self.fade_in < target {
            self.fade_in = (self.fade_in + dt * speed).min(target);
        } else if self.fade_in > target {
            self.fade_in = (self.fade_in - dt * speed).max(target);
        }

        let slide_target = if self.visible { 0.0 } else { -48.0 };
        self.slide += (slide_target - self.slide) * (dt * 10.0).min(1.0);
    }

    /// Gestisce un tasto premuto (port di `handleKeyPress`).
    pub fn handle_key_press(&mut self, key: i32) -> bool {
        if !self.visible && self.fade_in < 0.5 {
            return false;
        }

        let n = self.rows.len() as i32;
        match key {
            KEY_UP => {
                if n > 0 {
                    self.selected_row = (self.selected_row - 1 + n) % n;
                }
                true
            }
            KEY_DOWN => {
                if n > 0 {
                    self.selected_row = (self.selected_row + 1) % n;
                }
                true
            }
            KEY_LEFT => {
                self.adjust_selected(-1.0);
                true
            }
            KEY_RIGHT => {
                self.adjust_selected(1.0);
                true
            }
            KEY_TAB | 0x45 /* E */ | 0x65 /* e */ => {
                self.next_tab(1);
                true
            }
            0x51 /* Q */ | 0x71 /* q */ => {
                self.next_tab(-1);
                true
            }
            0xBB | 0x6B => {
                self.adjust_selected(1.0);
                true
            }
            0xBD | 0x6D => {
                self.adjust_selected(-1.0);
                true
            }
            KEY_ESC => {
                self.set_visible(false);
                true
            }
            _ => false,
        }
    }
}

// ── Persistenza per auto/pista ──────────────────────────────────────────────

/// Percorso del file di setup per auto/pista.
pub fn setup_path(root: &Path, car: &str, track: &str) -> PathBuf {
    root.join(sanitize(car))
        .join(format!("{}.ini", sanitize(track)))
}

fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Salva il setup in un file ini piatto. `false` se non scrivibile.
pub fn save_setup(path: &Path, setup: &SetupData) -> bool {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let text = format!(
        "# racingedge car setup\n\
         tirePressureFL={:.4}\ntirePressureFR={:.4}\n\
         tirePressureRL={:.4}\ntirePressureRR={:.4}\n\
         brakeBias={:.4}\nrideHeightFront={:.4}\nrideHeightRear={:.4}\n\
         springRateFront={:.4}\nspringRateRear={:.4}\n\
         frontWingAngle={:.4}\nrearWingAngle={:.4}\ndiffPreload={:.4}\n\
         fuel={:.4}\nballast={:.4}\ntcLevel={}\nabsLevel={}\n",
        setup.tire_pressure_fl,
        setup.tire_pressure_fr,
        setup.tire_pressure_rl,
        setup.tire_pressure_rr,
        setup.brake_bias,
        setup.ride_height_front,
        setup.ride_height_rear,
        setup.spring_rate_front,
        setup.spring_rate_rear,
        setup.front_wing_angle,
        setup.rear_wing_angle,
        setup.diff_preload,
        setup.fuel,
        setup.ballast,
        setup.tc_level,
        setup.abs_level,
    );
    std::fs::write(path, text).is_ok()
}

/// Carica il setup da un file ini piatto; `false` se il file non è leggibile
/// (il setup resta invariato). Le chiavi sconosciute vengono ignorate.
pub fn load_setup(path: &Path, setup: &mut SetupData) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    for line in text.lines() {
        let s = line.trim();
        if s.is_empty() || s.starts_with('#') {
            continue;
        }
        let Some(eq) = s.find('=') else {
            continue;
        };
        let value = s[eq + 1..].trim();
        match &s[..eq] {
            "tirePressureFL" => set_float(value, &mut setup.tire_pressure_fl),
            "tirePressureFR" => set_float(value, &mut setup.tire_pressure_fr),
            "tirePressureRL" => set_float(value, &mut setup.tire_pressure_rl),
            "tirePressureRR" => set_float(value, &mut setup.tire_pressure_rr),
            "brakeBias" => set_float(value, &mut setup.brake_bias),
            "rideHeightFront" => set_float(value, &mut setup.ride_height_front),
            "rideHeightRear" => set_float(value, &mut setup.ride_height_rear),
            "springRateFront" => set_float(value, &mut setup.spring_rate_front),
            "springRateRear" => set_float(value, &mut setup.spring_rate_rear),
            "frontWingAngle" => set_float(value, &mut setup.front_wing_angle),
            "rearWingAngle" => set_float(value, &mut setup.rear_wing_angle),
            "diffPreload" => set_float(value, &mut setup.diff_preload),
            "fuel" => set_float(value, &mut setup.fuel),
            "ballast" => set_float(value, &mut setup.ballast),
            "tcLevel" => set_int(value, &mut setup.tc_level),
            "absLevel" => set_int(value, &mut setup.abs_level),
            _ => {}
        }
    }
    true
}

fn set_float(value: &str, field: &mut f32) {
    if let Ok(v) = value.parse::<f32>() {
        *field = v;
    }
}

fn set_int(value: &str, field: &mut i32) {
    if let Ok(v) = value.parse::<i32>() {
        *field = v;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_setup_matches_reference() {
        let s = SetupData::default();
        assert_eq!(s.tire_pressure_fl, 2.20);
        assert_eq!(s.tire_pressure_rl, 2.00);
        assert_eq!(s.brake_bias, 0.56);
        assert_eq!(s.ride_height_front, 30.0);
        assert_eq!(s.ride_height_rear, 35.0);
        assert_eq!(s.front_wing_angle, 10.0);
        assert_eq!(s.rear_wing_angle, 12.0);
        assert_eq!(s.diff_preload, 30.0);
        assert_eq!(s.fuel, 50.0);
        assert_eq!(s.tc_level, 0);
    }

    #[test]
    fn rows_depend_on_tab() {
        let mut g = SetupGarage::new();
        assert_eq!(g.tab(), GarageTab::Overview);
        assert_eq!(g.rows().len(), 4);
        g.next_tab(1);
        assert_eq!(g.tab(), GarageTab::Tyres);
        assert_eq!(g.rows().len(), 4);
        g.next_tab(1);
        g.next_tab(1);
        assert_eq!(g.tab(), GarageTab::Aero);
        assert_eq!(g.rows().len(), 2);
    }

    #[test]
    fn tab_cycles_wrap_around() {
        let mut g = SetupGarage::new();
        g.next_tab(-1);
        assert_eq!(g.tab(), GarageTab::Gears);
        g.next_tab(1);
        assert_eq!(g.tab(), GarageTab::Overview);
    }

    #[test]
    fn adjust_clamps_float_and_int() {
        let mut g = SetupGarage::new();
        g.set_visible(true);
        g.update(1.0);
        // FUEL min 5, step 1.
        for _ in 0..200 {
            g.adjust_selected(-1.0);
        }
        assert_eq!(g.current_setup().fuel, 5.0);
        for _ in 0..1000 {
            g.adjust_selected(1.0);
        }
        assert_eq!(g.current_setup().fuel, 120.0);

        // Select TC (row 2) then clamp to 0..10.
        g.handle_key_press(KEY_DOWN);
        g.handle_key_press(KEY_DOWN);
        for _ in 0..50 {
            g.adjust_selected(1.0);
        }
        assert_eq!(g.current_setup().tc_level, 10);
        for _ in 0..50 {
            g.adjust_selected(-1.0);
        }
        assert_eq!(g.current_setup().tc_level, 0);
    }

    #[test]
    fn selection_wraps_with_arrows() {
        let mut g = SetupGarage::new();
        g.set_visible(true);
        g.update(1.0);
        g.handle_key_press(KEY_UP);
        assert_eq!(g.selected_row(), 3);
        g.handle_key_press(KEY_DOWN);
        assert_eq!(g.selected_row(), 0);
    }

    #[test]
    fn key_press_ignored_when_hidden_and_faded() {
        let mut g = SetupGarage::new();
        assert!(!g.handle_key_press(KEY_DOWN));
        g.set_visible(true);
        g.update(1.0);
        assert!(g.handle_key_press(KEY_DOWN));
    }

    #[test]
    fn esc_hides_and_fires_callback() {
        let mut g = SetupGarage::new();
        let flag = std::rc::Rc::new(std::cell::Cell::new(false));
        let f2 = std::rc::Rc::clone(&flag);
        g.set_on_closed(move || f2.set(true));
        g.set_visible(true);
        g.update(1.0);
        assert!(g.handle_key_press(KEY_ESC));
        assert!(!g.is_visible());
        assert!(flag.get());
    }

    #[test]
    fn setup_changed_callback_receives_data() {
        let mut g = SetupGarage::new();
        let seen = std::rc::Rc::new(std::cell::Cell::new(0.0_f32));
        let s2 = std::rc::Rc::clone(&seen);
        g.set_on_setup_changed(move |data| s2.set(data.fuel));
        g.adjust_selected(1.0);
        assert_eq!(seen.get(), 51.0);
    }

    #[test]
    fn fade_animates_toward_target() {
        let mut g = SetupGarage::new();
        assert_eq!(g.fade_alpha(), 0.0);
        g.set_visible(true);
        g.update(0.1);
        assert!(g.fade_alpha() > 0.0 && g.fade_alpha() < 1.0);
        g.update(1.0);
        assert_eq!(g.fade_alpha(), 1.0);
        g.set_visible(false);
        g.update(1.0);
        assert_eq!(g.fade_alpha(), 0.0);
    }

    #[test]
    fn normalized_tracks_range() {
        let g = SetupGarage::new();
        let fuel_row = g.rows()[0];
        let mut data = SetupData {
            fuel: 5.0,
            ..SetupData::default()
        };
        assert_eq!(fuel_row.normalized(&data), 0.0);
        data.fuel = 120.0;
        assert_eq!(fuel_row.normalized(&data), 1.0);
    }

    #[test]
    fn params_derive_from_setup() {
        let data = SetupData {
            tire_pressure_fl: 2.1,
            tire_pressure_rr: 1.9,
            brake_bias: 0.6,
            front_wing_angle: 14.0,
            rear_wing_angle: 18.0,
            ballast: 20.0,
            fuel: 33.0,
            ..SetupData::default()
        };
        let params = SetupParams::from_setup_data(&data);
        assert_eq!(params.tire_pressure, [2.1, 2.2, 2.0, 1.9]);
        assert_eq!(params.brake_bias, 0.6);
        assert_eq!(params.front_wing, 14.0);
        assert_eq!(params.rear_wing, 18.0);
        assert_eq!(params.ballast, 20.0);
        assert_eq!(params.fuel, 33.0);
        assert_eq!(params.camber[0], -2.5);
    }

    #[test]
    fn setup_path_sanitizes_names() {
        let path = setup_path(Path::new("setups"), "Ferrari 488", "Spa GP/24h");
        assert!(path.ends_with(Path::new("Ferrari_488/Spa_GP_24h.ini")));
    }

    #[test]
    fn save_load_setup_round_trip() {
        let dir = std::env::temp_dir().join("racingedge_setup_test");
        let path = setup_path(&dir, "TestCar", "TestTrack");
        let original = SetupData {
            fuel: 77.5,
            tc_level: 4,
            brake_bias: 0.615,
            ..SetupData::default()
        };
        assert!(save_setup(&path, &original));

        let mut loaded = SetupData::default();
        assert!(load_setup(&path, &mut loaded));
        assert_eq!(loaded.fuel, 77.5);
        assert_eq!(loaded.tc_level, 4);
        assert!((loaded.brake_bias - 0.615).abs() < 1e-3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_missing_setup_returns_false() {
        let mut data = SetupData::default();
        let missing = std::env::temp_dir().join("racingedge_setup_test/absent.ini");
        assert!(!load_setup(&missing, &mut data));
        assert_eq!(data.fuel, 50.0);
    }
}
