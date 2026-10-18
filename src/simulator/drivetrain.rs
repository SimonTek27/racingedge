//! Cambio, final drive, rapporto totale, slip ratio.

#[derive(Clone, Debug)]
pub struct DrivetrainConfig {
    pub gear_ratios: Vec<f32>,
    pub final_drive: f32,
    pub efficiency: f32,
}

impl Default for DrivetrainConfig {
    fn default() -> Self {
        Self {
            gear_ratios: vec![2.6, 2.0, 1.7, 1.4, 1.2, 1.1, 1.0],
            final_drive: 3.2,
            efficiency: 0.92,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DrivetrainState {
    /// -1 = R, 0 = N, 1..N = marce
    pub gear: i8,
}

impl Default for DrivetrainState {
    fn default() -> Self {
        Self { gear: 1 }
    }
}

impl DrivetrainConfig {
    pub fn gear_ratio(&self, gear: i8) -> f32 {
        match gear {
            -1 => -self.gear_ratios.first().copied().unwrap_or(2.5) * 0.85,
            1..=8 => self
                .gear_ratios
                .get((gear - 1) as usize)
                .copied()
                .unwrap_or(1.0),
            _ => 0.0,
        }
    }

    pub fn total_ratio(&self, gear: i8) -> f32 {
        self.gear_ratio(gear) * self.final_drive
    }

    /// Forza longitudinale richiesta alle ruote da coppia motore [N].
    pub fn drive_force(&self, engine_torque_nm: f32, gear: i8, wheel_radius: f32) -> f32 {
        let total = self.total_ratio(gear);
        let wheel_tq = engine_torque_nm * total.abs() * self.efficiency * total.signum();
        wheel_tq / wheel_radius.max(0.05)
    }
}

/// Slip ratio κ = (ω·r − vx) / max(|vx|, ε)
#[inline]
pub fn slip_ratio(vx: f32, wheel_omega: f32, radius: f32) -> f32 {
    let vx_abs = vx.abs().max(0.5);
    let vs = wheel_omega * radius - vx;
    (vs / vx_abs).clamp(-1.5, 1.5)
}

#[inline]
pub fn wheel_omega_from_engine(rpm: f32, total_ratio: f32) -> f32 {
    if total_ratio.abs() < 0.01 {
        return 0.0;
    }
    (rpm * std::f32::consts::TAU / 60.0) / total_ratio.abs()
}

// ── Differenziale (port di ksengine DifferentialModel.*) ────────────────────

/// Tipo di differenziale (port di `DifferentialModel::DiffType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DiffType {
    /// Differenziale aperto.
    Open,
    /// LSD a dischi (clutch).
    #[default]
    LsdCls,
    /// LSD viscoso.
    LsdViscous,
    /// LSD a ingranaggi (Torsen).
    LsdGeared,
    /// Bloccato.
    Locked,
    /// Attivo/controllato.
    Active,
}

/// Configurazione del differenziale (port di `DiffConfig`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiffConfig {
    /// Tipo di differenziale.
    pub diff_type: DiffType,
    /// Precarico, in Nm.
    pub preload: f32,
    /// Rampa in rilascio (coast).
    pub coast_power: f32,
    /// Rampa in trazione (drive).
    pub drive_power: f32,
    /// Coppia di bloccaggio massima, in Nm.
    pub max_lock: f32,
    /// Soglia di slittamento.
    pub slip_threshold: f32,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            diff_type: DiffType::LsdCls,
            preload: 20.0,
            coast_power: 0.3,
            drive_power: 0.5,
            max_lock: 400.0,
            slip_threshold: 0.05,
        }
    }
}

/// Stato del differenziale (port di `DiffState`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DiffState {
    /// Coppia alla ruota sinistra, in Nm.
    pub left_torque: f32,
    /// Coppia alla ruota destra, in Nm.
    pub right_torque: f32,
    /// Coppia di bloccaggio corrente, in Nm.
    pub locking_torque: f32,
    /// Slittamento corrente tra le due ruote.
    pub slip_ratio: f32,
    /// Temperatura, in °C.
    pub temperature: f32,
    /// Se il differenziale sta bloccando.
    pub is_locking: bool,
}

impl Default for DiffState {
    fn default() -> Self {
        Self {
            left_torque: 0.0,
            right_torque: 0.0,
            locking_torque: 0.0,
            slip_ratio: 0.0,
            temperature: 80.0,
            is_locking: false,
        }
    }
}

/// Differenziale aperto/LSD/bloccato/attivo (port di `DifferentialModel`).
#[derive(Clone, Debug)]
pub struct DifferentialModel {
    config: DiffConfig,
    state: DiffState,
}

impl Default for DifferentialModel {
    fn default() -> Self {
        Self::with_config(Self::lsd_clutch())
    }
}

impl DifferentialModel {
    /// Un differenziale con la configurazione predefinita (LSD a dischi).
    pub fn new() -> Self {
        Self::default()
    }

    /// Un differenziale con una configurazione esplicita.
    pub fn with_config(config: DiffConfig) -> Self {
        Self {
            config,
            state: DiffState::default(),
        }
    }

    /// Azzera lo stato (temperatura a 80 °C), come `reset`.
    pub fn reset(&mut self) {
        self.state = DiffState::default();
        self.state.temperature = 80.0;
    }

    /// Imposta la configurazione.
    pub fn set_config(&mut self, config: DiffConfig) {
        self.config = config;
    }

    /// La configurazione corrente.
    pub fn config(&self) -> DiffConfig {
        self.config
    }

    /// Lo stato corrente.
    pub fn state(&self) -> DiffState {
        self.state
    }

    /// Coppia alla ruota sinistra.
    pub fn left_torque(&self) -> f32 {
        self.state.left_torque
    }

    /// Coppia alla ruota destra.
    pub fn right_torque(&self) -> f32 {
        self.state.right_torque
    }

    /// Slip ratio tra le due ruote (port di `calculateSlipRatio`).
    pub fn calculate_slip_ratio(&self, left_speed: f32, right_speed: f32) -> f32 {
        let avg = 0.5 * (left_speed + right_speed);
        if avg.abs() < 0.1 {
            return 0.0;
        }
        (left_speed - right_speed) / avg
    }

    /// Coppia di bloccaggio per lo slip dato (port di `calculateLockingTorque`).
    pub fn calculate_locking_torque(&self, slip_ratio: f32, is_drive: bool) -> f32 {
        let abs_slip = slip_ratio.abs();
        match self.config.diff_type {
            DiffType::Open => 0.0,
            DiffType::Locked => self.config.max_lock,
            DiffType::LsdViscous => {
                (self.config.preload + abs_slip * 800.0).min(self.config.max_lock)
            }
            DiffType::LsdGeared => {
                (self.config.preload + abs_slip * 1200.0).min(self.config.max_lock)
            }
            DiffType::Active => (self.config.preload + abs_slip * 1500.0).min(self.config.max_lock),
            DiffType::LsdCls => {
                let power = if is_drive {
                    self.config.drive_power
                } else {
                    self.config.coast_power
                };
                let lock = self.config.preload + power * abs_slip * self.config.max_lock;
                lock.clamp(0.0, self.config.max_lock)
            }
        }
    }

    /// Un passo di differenziale: ripartisce `input_torque` e aggiorna la
    /// temperatura (port di `update`).
    pub fn update(&mut self, dt: f32, input_torque: f32, left_speed: f32, right_speed: f32) {
        let dt = dt.clamp(1e-4, 0.05);
        let slip = self.calculate_slip_ratio(left_speed, right_speed);
        self.state.slip_ratio = slip;

        let is_drive = input_torque > 1.0;
        let lock = self.calculate_locking_torque(slip, is_drive);
        self.state.locking_torque = lock;
        self.state.is_locking = lock > self.config.preload * 1.1;

        let half = input_torque * 0.5;
        let mut transfer = 0.0;
        if self.config.diff_type != DiffType::Open {
            transfer = (input_torque.abs() * 0.5).min(lock).copysign(-slip);
        }
        self.state.left_torque = half - transfer;
        self.state.right_torque = half + transfer;

        self.state.temperature +=
            (lock.abs() * 0.001 - (self.state.temperature - 40.0) * 0.02) * dt;
        self.state.temperature = self.state.temperature.clamp(40.0, 180.0);
    }

    /// Legge `PRELOAD`/`POWER`/`COAST` da un INI piatto (port di `loadFromIni`).
    pub fn load_from_ini(&mut self, path: impl AsRef<std::path::Path>) -> Result<(), String> {
        let ini = crate::editor::ini::Ini::load(path)?;
        let Some(root) = ini.section("_ROOT") else {
            return Ok(());
        };
        for (key, value) in root.iter() {
            if key.contains("PRELOAD") {
                if let Some(v) = crate::editor::ini::parse_f32(value) {
                    self.config.preload = v;
                }
            } else if key.contains("POWER") && !key.contains("COAST") {
                if let Some(v) = crate::editor::ini::parse_f32(value) {
                    self.config.drive_power = v;
                }
            } else if key.contains("COAST") {
                if let Some(v) = crate::editor::ini::parse_f32(value) {
                    self.config.coast_power = v;
                }
            }
        }
        Ok(())
    }

    /// Preset: differenziale aperto.
    pub fn open() -> DiffConfig {
        DiffConfig {
            diff_type: DiffType::Open,
            preload: 0.0,
            max_lock: 0.0,
            ..Default::default()
        }
    }

    /// Preset: LSD a dischi.
    pub fn lsd_clutch() -> DiffConfig {
        DiffConfig {
            diff_type: DiffType::LsdCls,
            ..Default::default()
        }
    }

    /// Preset: LSD viscoso.
    pub fn lsd_viscous() -> DiffConfig {
        DiffConfig {
            diff_type: DiffType::LsdViscous,
            ..Default::default()
        }
    }

    /// Preset: LSD Torsen.
    pub fn lsd_torsen() -> DiffConfig {
        DiffConfig {
            diff_type: DiffType::LsdGeared,
            preload: 10.0,
            ..Default::default()
        }
    }

    /// Preset: bloccato.
    pub fn locked() -> DiffConfig {
        DiffConfig {
            diff_type: DiffType::Locked,
            max_lock: 2000.0,
            ..Default::default()
        }
    }

    /// Preset: attivo.
    pub fn active() -> DiffConfig {
        DiffConfig {
            diff_type: DiffType::Active,
            ..Default::default()
        }
    }

    /// Nome leggibile del tipo (port di `getDiffTypeName`).
    pub fn diff_type_name(diff_type: DiffType) -> &'static str {
        match diff_type {
            DiffType::Open => "Open",
            DiffType::LsdCls => "LSD Clutch",
            DiffType::LsdViscous => "LSD Viscous",
            DiffType::LsdGeared => "LSD Torsen",
            DiffType::Locked => "Locked",
            DiffType::Active => "Active",
        }
    }
}

// ── Cambio (port di ksengine GearboxModel) ──────────────────────────────────

/// Cambio con rapporto finale e folle/retromarcia (port di `GearboxModel`).
#[derive(Clone, Debug)]
pub struct GearboxModel {
    ratios: Vec<f32>,
    final_drive: f32,
    gear: i32,
}

impl Default for GearboxModel {
    fn default() -> Self {
        Self {
            ratios: vec![3.5, 2.2, 1.6, 1.25, 1.0, 0.85],
            final_drive: 3.8,
            gear: 1,
        }
    }
}

impl GearboxModel {
    /// Un cambio con i rapporti predefiniti.
    pub fn new() -> Self {
        Self::default()
    }

    /// Imposta rapporti e rapporto finale (port di `setRatios`).
    pub fn set_ratios(&mut self, ratios: Vec<f32>, final_drive: f32) {
        if !ratios.is_empty() {
            self.ratios = ratios;
        }
        self.final_drive = final_drive;
        let count = self.ratios.len() as i32;
        if self.gear > count {
            self.gear = count;
        }
        if self.gear < 0 {
            self.gear = 0;
        }
    }

    /// Seleziona una marcia (0 = folle), con clamp sul numero di rapporti.
    pub fn set_gear(&mut self, gear: i32) {
        let count = self.ratios.len() as i32;
        self.gear = gear.clamp(0, count);
    }

    /// La marcia corrente (0 = folle).
    pub fn gear(&self) -> i32 {
        self.gear
    }

    /// Il rapporto finale.
    pub fn final_drive(&self) -> f32 {
        self.final_drive
    }

    /// Il rapporto totale corrente, `0` in folle (port di `currentRatio`).
    pub fn current_ratio(&self) -> f32 {
        if self.gear <= 0 || self.gear > self.ratios.len() as i32 {
            return 0.0;
        }
        self.ratios[self.gear as usize - 1] * self.final_drive
    }

    /// Scala marcia automaticamente in base al regime (port di `autoShift`).
    pub fn auto_shift(&mut self, rpm: f32, max_rpm: f32, idle_rpm: f32) {
        if self.ratios.is_empty() {
            return;
        }
        let count = self.ratios.len() as i32;
        if rpm > max_rpm * 0.92 && self.gear < count {
            self.gear += 1;
        } else if rpm < idle_rpm * 1.8 && self.gear > 1 {
            self.gear -= 1;
        }
    }

    /// Coppia alle ruote dalla coppia motore (port di `wheelTorqueFromEngine`).
    pub fn wheel_torque_from_engine(&self, engine_torque: f32) -> f32 {
        engine_torque * self.current_ratio()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_diff_transfers_nothing() {
        let mut diff = DifferentialModel::with_config(DifferentialModel::open());
        diff.update(0.016, 200.0, 10.0, 8.0);
        assert_eq!(diff.left_torque(), 100.0);
        assert_eq!(diff.right_torque(), 100.0);
        assert_eq!(diff.state().locking_torque, 0.0);
    }

    #[test]
    fn locked_diff_transfers_torque_opposing_slip() {
        // Port faithful: left faster (slip > 0) → negative transfer, therefore the
        // ruota sinistra riceve in più (400) e la destra in meno (0), come nel
        // riferimento `transfer = copysign(min(lock, |T|*0.5), -slip)`.
        let mut diff = DifferentialModel::with_config(DifferentialModel::locked());
        diff.update(0.016, 400.0, 12.0, 8.0);
        assert_eq!(diff.left_torque(), 400.0);
        assert_eq!(diff.right_torque(), 0.0);
        assert!(diff.state().is_locking);
    }

    #[test]
    fn lsd_clutch_locking_torque_tracks_slip() {
        let diff = DifferentialModel::with_config(DifferentialModel::lsd_clutch());
        let low = diff.calculate_locking_torque(0.01, true);
        let high = diff.calculate_locking_torque(0.05, true);
        assert!(high > low);
        assert!(high <= diff.config().max_lock);
    }

    #[test]
    fn locking_torque_is_capped_at_max_lock() {
        let diff = DifferentialModel::with_config(DifferentialModel::lsd_viscous());
        assert_eq!(
            diff.calculate_locking_torque(1.0, true),
            diff.config().max_lock
        );
    }

    #[test]
    fn slip_ratio_is_zero_at_low_speed() {
        let diff = DifferentialModel::default();
        assert_eq!(diff.calculate_slip_ratio(0.05, -0.05), 0.0);
        assert!((diff.calculate_slip_ratio(10.0, 8.0) - 2.0 / 9.0).abs() < 1e-5);
    }

    #[test]
    fn temperature_stays_in_range() {
        let mut diff = DifferentialModel::with_config(DifferentialModel::locked());
        for _ in 0..10_000 {
            diff.update(0.05, 500.0, 20.0, 5.0);
        }
        let t = diff.state().temperature;
        assert!((40.0..=180.0).contains(&t), "temperatura {t}");
    }

    #[test]
    fn diff_type_names_match_reference() {
        assert_eq!(DifferentialModel::diff_type_name(DiffType::Open), "Open");
        assert_eq!(
            DifferentialModel::diff_type_name(DiffType::LsdGeared),
            "LSD Torsen"
        );
        assert_eq!(
            DifferentialModel::diff_type_name(DiffType::Active),
            "Active"
        );
    }

    #[test]
    fn reset_restores_initial_state() {
        let mut diff = DifferentialModel::default();
        diff.update(0.016, 500.0, 20.0, 5.0);
        diff.reset();
        assert_eq!(diff.state().temperature, 80.0);
        assert_eq!(diff.left_torque(), 0.0);
    }

    #[test]
    fn gearbox_current_ratio_uses_final_drive() {
        let mut gearbox = GearboxModel::new();
        gearbox.set_ratios(vec![3.0, 2.0, 1.5], 4.0);
        gearbox.set_gear(2);
        assert_eq!(gearbox.current_ratio(), 8.0);
        gearbox.set_gear(0);
        assert_eq!(gearbox.current_ratio(), 0.0);
    }

    #[test]
    fn gearbox_clamps_gear() {
        let mut gearbox = GearboxModel::new();
        gearbox.set_ratios(vec![3.0, 2.0], 4.0);
        gearbox.set_gear(9);
        assert_eq!(gearbox.gear(), 2);
        gearbox.set_gear(-5);
        assert_eq!(gearbox.gear(), 0);
    }

    #[test]
    fn gearbox_auto_shift_up_and_down() {
        let mut gearbox = GearboxModel::new();
        gearbox.set_gear(1);
        gearbox.auto_shift(7800.0, 8000.0, 1000.0);
        assert_eq!(gearbox.gear(), 2);
        gearbox.auto_shift(1000.0, 8000.0, 1000.0);
        assert_eq!(gearbox.gear(), 1);
        // At the top gear, shifting up does nothing.
        gearbox.set_gear(gearbox.ratios.len() as i32);
        gearbox.auto_shift(9000.0, 8000.0, 1000.0);
        assert_eq!(gearbox.gear(), gearbox.ratios.len() as i32);
    }

    #[test]
    fn gearbox_wheel_torque_uses_ratio() {
        let mut gearbox = GearboxModel::new();
        gearbox.set_ratios(vec![2.0], 3.0);
        gearbox.set_gear(1);
        assert_eq!(gearbox.wheel_torque_from_engine(100.0), 600.0);
    }
}
