//! Danni in stile rFactor2: zone, componenti, impatti, usura meccanica e
//! telemetria.
//!
//! Port di `src/engine/physics/DamageSystem.{h,cpp}`, `Rf2DamageModel.h`,
//! `MechanicalDamage.h` e `DamageTelemetry.h` di ksengine.

use std::ops::{BitAnd, BitOr};

use bevy::math::Vec3;

// ── Zone di danno (DamageZone) ──────────────────────────────────────────────

/// Zona fisica della vettura che può subire danni (port di `DamageZone`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(usize)]
pub enum DamageZone {
    /// Anteriore sinistra.
    #[default]
    FrontLeft = 0,
    /// Anteriore centrale.
    FrontCenter,
    /// Anteriore destra.
    FrontRight,
    /// Posteriore sinistra.
    RearLeft,
    /// Posteriore centrale.
    RearCenter,
    /// Posteriore destra.
    RearRight,
    /// Fiancata sinistra.
    LeftSide,
    /// Fiancata destra.
    RightSide,
    /// Vano motore.
    EngineBay,
    /// Abitacolo.
    Cabin,
    /// Sottoscocca.
    Underbody,
}

impl DamageZone {
    /// Numero di zone (port di `DamageZone::COUNT`).
    pub const COUNT: usize = 11;

    /// L'indice della zona.
    pub fn index(self) -> usize {
        self as usize
    }
}

// ── Tipi di danno (DamageType) ──────────────────────────────────────────────

/// Bitmask dei sistemi danneggiabili (port di `DamageType`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DamageType(pub u32);

impl DamageType {
    /// Nessun danno.
    pub const NONE: Self = Self(0);
    /// Pannello di carrozzeria.
    pub const BODY_PANEL: Self = Self(1 << 0);
    /// Sospensione.
    pub const SUSPENSION: Self = Self(1 << 1);
    /// Aerodinamica.
    pub const AERO: Self = Self(1 << 2);
    /// Motore.
    pub const ENGINE: Self = Self(1 << 3);
    /// Trasmissione.
    pub const TRANSMISSION: Self = Self(1 << 4);
    /// Raffreddamento.
    pub const COOLING: Self = Self(1 << 5);
    /// Impianto carburante.
    pub const FUEL_SYSTEM: Self = Self(1 << 6);
    /// Freni.
    pub const BRAKES: Self = Self(1 << 7);
    /// Sterzo.
    pub const STEERING: Self = Self(1 << 8);
    /// Scarico.
    pub const EXHAUST: Self = Self(1 << 9);
    /// Impianto elettrico.
    pub const ELECTRICAL: Self = Self(1 << 10);

    /// `true` se `self` contiene il bit di `flag` (port di `hasFlag`).
    pub fn has_flag(self, flag: Self) -> bool {
        (self.0 & flag.0) != 0
    }

    /// `true` se `self` interseca `other`.
    pub fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
}

impl BitOr for DamageType {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl BitAnd for DamageType {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}

// ── Dati per zona (DamageZoneData) ──────────────────────────────────────────

/// Danno di una zona (port di `DamageZoneData`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DamageZoneData {
    /// Integrità strutturale (`0` = distrutto, `1` = perfetto).
    pub structural: f32,
    /// Danno estetico (`0` = perfetto, `1` = distrutto).
    pub cosmetic: f32,
    /// Deformazione visiva (`0..=1`).
    pub deformation: f32,
    /// Direzione della deformazione.
    pub deformation_dir: Vec3,
    /// Energia d'impatto accumulata, J.
    pub impact_energy: f32,
    /// Tipi di danno presenti.
    pub damage_types: DamageType,
    /// Numero di impatti sulla zona.
    pub impact_count: i32,
}

impl DamageZoneData {
    /// Applica un impatto alla zona (port di `applyImpact`).
    pub fn apply_impact(&mut self, energy: f32, direction: Vec3, damage_type: DamageType) {
        self.impact_energy += energy;
        self.impact_count += 1;
        self.damage_types = self.damage_types | damage_type;

        let normalized_energy = (energy / 50000.0).min(1.0);

        self.structural = (self.structural - normalized_energy * 0.3).max(0.0);
        self.cosmetic = (self.cosmetic + normalized_energy * 0.5).min(1.0);
        self.deformation = (self.deformation + normalized_energy * 0.4).min(1.0);

        self.deformation_dir = direction.normalize_or_zero();
    }

    /// Danno combinato (`0` = perfetto, `1` = distrutto), port di `combinedDamage`.
    pub fn combined_damage(&self) -> f32 {
        ((1.0 - self.structural) * 0.7 + self.cosmetic * 0.3).clamp(0.0, 1.0)
    }

    /// Integrità strutturale (port di `integrity`).
    pub fn integrity(&self) -> f32 {
        self.structural
    }
}

// ── Danno sospensioni (SuspensionDamageData) ────────────────────────────────

/// Danno di una sospensione (port di `SuspensionDamageData`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SuspensionDamageData {
    /// Integrità della geometria (`1` = allineamento perfetto).
    pub geometry: f32,
    /// Resistenza del braccio (`1` = perfetto).
    pub arm_strength: f32,
    /// Perdita di smorzamento (`0` = nessuna).
    pub damping_loss: f32,
    /// Degrado della molla (`0` = nessuno).
    pub spring_damage: f32,
    /// Deviazione di convergenza, gradi.
    pub toe_deviation: f32,
    /// Deviazione di campanatura, gradi.
    pub camber_deviation: f32,
    /// Se la sospensione è completamente rotta.
    pub is_broken: bool,
}

impl Default for SuspensionDamageData {
    fn default() -> Self {
        Self {
            geometry: 1.0,
            arm_strength: 1.0,
            damping_loss: 0.0,
            spring_damage: 0.0,
            toe_deviation: 0.0,
            camber_deviation: 0.0,
            is_broken: false,
        }
    }
}

impl SuspensionDamageData {
    /// Moltiplicatore di handling (port di `handlingMultiplier`).
    pub fn handling_multiplier(&self) -> f32 {
        if self.is_broken {
            return 0.0;
        }
        let mut mult = 1.0;
        mult *= self.geometry;
        mult *= self.arm_strength;
        mult *= 1.0 - self.damping_loss * 0.5;
        mult *= 1.0 - self.spring_damage * 0.3;

        let deviation_factor = self.toe_deviation.abs() / 5.0 + self.camber_deviation.abs() / 5.0;
        mult *= (1.0 - deviation_factor).max(0.3);

        mult.clamp(0.0, 1.0)
    }
}

// ── Danno motore (EngineDamageData) ─────────────────────────────────────────

/// Danno del motore (port di `EngineDamageData`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineDamageData {
    /// Salute complessiva (`0` = distrutto).
    pub health: f32,
    /// Fattore di perdita di potenza (`0` = nessuna).
    pub power_loss: f32,
    /// Danno da surriscaldamento (`0..=1`).
    pub overheating: f32,
    /// Perdita di pressione dell'olio (`0..=1`).
    pub oil_pressure_loss: f32,
    /// Perdita di liquido refrigerante, litri/min.
    pub coolant_leak: f32,
    /// Frequenza di mancate accensioni (`0..=1`).
    pub misfire_rate: f32,
    /// Se il motore è completamente grippato.
    pub is_seized: bool,
}

impl Default for EngineDamageData {
    fn default() -> Self {
        Self {
            health: 1.0,
            power_loss: 0.0,
            overheating: 0.0,
            oil_pressure_loss: 0.0,
            coolant_leak: 0.0,
            misfire_rate: 0.0,
            is_seized: false,
        }
    }
}

impl EngineDamageData {
    /// Potenza effettiva (port di `effectivePowerMultiplier`).
    pub fn effective_power_multiplier(&self) -> f32 {
        if self.is_seized {
            return 0.0;
        }
        let mut mult = self.health;
        mult *= 1.0 - self.power_loss;
        mult *= 1.0 - self.overheating * 0.4;
        mult *= 1.0 - self.misfire_rate * 0.3;
        mult.clamp(0.05, 1.0)
    }

    /// Aumento del consumo di carburante (port di `fuelConsumptionIncrease`).
    pub fn fuel_consumption_increase(&self) -> f32 {
        let mut increase = 0.0;
        increase += self.overheating * 0.2;
        increase += self.misfire_rate * 0.15;
        increase += (1.0 - self.health) * 0.1;
        increase.clamp(0.0, 0.5)
    }
}

// ── Danno aerodinamica (AeroDamageData) ─────────────────────────────────────

/// Danno aerodinamico (port di `AeroDamageData`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AeroDamageData {
    /// Danno all'ala anteriore (`0` = perfetta).
    pub front_wing_damage: f32,
    /// Danno all'ala posteriore (`0` = perfetta).
    pub rear_wing_damage: f32,
    /// Danno al diffusore (`0..=1`).
    pub diffuser_damage: f32,
    /// Danno al fondo (`0..=1`).
    pub floor_damage: f32,
    /// Danno al radiatore (`0..=1`).
    pub radiator_damage: f32,
}

impl AeroDamageData {
    /// Perdita di deportanza (port di `downforceMultiplier`).
    pub fn downforce_multiplier(&self) -> f32 {
        let mut mult = 1.0;
        mult -= self.front_wing_damage * 0.35;
        mult -= self.rear_wing_damage * 0.40;
        mult -= self.diffuser_damage * 0.20;
        mult -= self.floor_damage * 0.15;
        mult.clamp(0.1, 1.0)
    }

    /// Aumento di resistenza (port di `dragMultiplier`).
    pub fn drag_multiplier(&self) -> f32 {
        let mut mult = 1.0;
        mult += self.front_wing_damage * 0.15;
        mult += self.rear_wing_damage * 0.10;
        mult += self.floor_damage * 0.20;
        mult.clamp(1.0, 2.0)
    }

    /// Efficienza di raffreddamento (port di `coolingEfficiency`).
    pub fn cooling_efficiency(&self) -> f32 {
        let mut eff = 1.0;
        eff -= self.radiator_damage * 0.6;
        eff -= self.floor_damage * 0.1;
        eff.clamp(0.2, 1.0)
    }
}

// ── Danno trasmissione (TransmissionDamageData) ─────────────────────────────

/// Danno della trasmissione (port di `TransmissionDamageData`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TransmissionDamageData {
    /// Salute complessiva.
    pub health: f32,
    /// Danno per marcia (`0` = perfetta, `1` = distrutta).
    pub gear_damage: [f32; 8],
    /// Usura della frizione (`0..=1`).
    pub clutch_damage: f32,
    /// Danno al differenziale (`0..=1`).
    pub diff_damage: f32,
    /// Se la trasmissione è bloccata in marcia.
    pub is_stuck: bool,
}

impl Default for TransmissionDamageData {
    fn default() -> Self {
        Self {
            health: 1.0,
            gear_damage: [0.0; 8],
            clutch_damage: 0.0,
            diff_damage: 0.0,
            is_stuck: false,
        }
    }
}

impl TransmissionDamageData {
    /// Moltiplicatore di efficienza (port di `efficiencyMultiplier`).
    pub fn efficiency_multiplier(&self) -> f32 {
        if self.is_stuck {
            return 0.5;
        }
        let mut mult = self.health;
        let worst_gear_damage = self.gear_damage.iter().copied().fold(0.0_f32, f32::max);
        mult *= 1.0 - worst_gear_damage * 0.3;
        mult *= 1.0 - self.clutch_damage * 0.2;
        mult *= 1.0 - self.diff_damage * 0.2;
        mult.clamp(0.3, 1.0)
    }
}

// ── Danno freni (BrakeDamageData) ───────────────────────────────────────────

/// Danno dei freni di un angolo (port di `BrakeDamageData`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BrakeDamageData {
    /// Danno al disco (`0..=1`).
    pub disc_damage: f32,
    /// Usura delle pastiglie (`0` = nuove, `1` = finite).
    pub pad_wear: f32,
    /// Danno alla pinza (`0..=1`).
    pub caliper_damage: f32,
    /// Livello di fade corrente (`0..=1`).
    pub fade_level: f32,
    /// Temperatura corrente, °C.
    pub temperature: f32,
    /// Se il freno è in fade.
    pub is_faded: bool,
}

impl Default for BrakeDamageData {
    fn default() -> Self {
        Self {
            disc_damage: 0.0,
            pad_wear: 0.0,
            caliper_damage: 0.0,
            fade_level: 0.0,
            temperature: 300.0,
            is_faded: false,
        }
    }
}

impl BrakeDamageData {
    /// Efficacia frenante (port di `brakingMultiplier`).
    pub fn braking_multiplier(&self) -> f32 {
        let mut mult = 1.0;
        mult *= 1.0 - self.disc_damage * 0.4;
        mult *= 1.0 - self.pad_wear * 0.6;
        mult *= 1.0 - self.caliper_damage * 0.5;
        mult *= 1.0 - self.fade_level * 0.3;
        mult.clamp(0.1, 1.0)
    }

    /// Vita residua delle pastiglie (port di `padRemaining`).
    pub fn pad_remaining(&self) -> f32 {
        1.0 - self.pad_wear
    }
}

// ── Evento di collisione (CollisionEvent) ───────────────────────────────────

/// Collisione da elaborare (port di `CollisionEvent`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CollisionEvent {
    /// Punto di contatto in spazio mondo.
    pub contact_point: Vec3,
    /// Normale di contatto (direzione dell'impatto).
    pub contact_normal: Vec3,
    /// Velocità relativa all'impatto.
    pub impact_velocity: Vec3,
    /// Energia cinetica trasferita, J.
    pub impact_energy: f32,
    /// Forza di picco, N.
    pub impact_force: f32,
    /// Zona primaria del danno.
    pub primary_zone: DamageZone,
    /// Tipo di danno.
    pub damage_type: DamageType,
    /// Istante della collisione.
    pub timestamp: f32,
}

// ── Dati di riparazione (RepairData) ────────────────────────────────────────

/// Tempi e costi di riparazione (port di `RepairData`).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RepairData {
    /// Tempo per riparare la carrozzeria, s.
    pub body_repair_time: f32,
    /// Tempo per riparare le sospensioni, s.
    pub suspension_repair_time: f32,
    /// Tempo per riparare il motore, s (`0` = sostituzione).
    pub engine_repair_time: f32,
    /// Tempo per riparare l'aerodinamica, s.
    pub aero_repair_time: f32,
    /// Costo totale di riparazione.
    pub total_repair_cost: f32,
    /// Parti che richiedono sostituzione.
    pub needs_replacement: [bool; DamageZone::COUNT],
}

impl RepairData {
    /// Tempo totale di riparazione (port di `totalTime`).
    pub fn total_time(&self) -> f32 {
        self.body_repair_time
            + self.suspension_repair_time
            + self.engine_repair_time
            + self.aero_repair_time
    }
}

// ── Configurazione (DamageConfig) ───────────────────────────────────────────

/// Configurazione del sistema danni (port di `DamageConfig`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageConfig {
    /// Se il sistema è attivo.
    pub enabled: bool,
    /// Moltiplicatore globale dei danni.
    pub global_damage_multiplier: f32,
    /// Moltiplicatore della deformazione visiva.
    pub visual_damage_multiplier: f32,
    /// Moltiplicatore dell'impatto fisico.
    pub physics_damage_multiplier: f32,
    /// Moltiplicatore dell'usura dei componenti.
    pub wear_multiplier: f32,
    /// Energia minima per causare un danno, J.
    pub impact_threshold: f32,
    /// Danno strutturale massimo prima del cedimento.
    pub max_structural_damage: f32,
    /// Se consentire riparazioni parziali.
    pub allow_partial_repairs: bool,
    /// Costo per punto di danno.
    pub repair_cost_per_point: f32,
}

impl Default for DamageConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            global_damage_multiplier: 1.0,
            visual_damage_multiplier: 1.0,
            physics_damage_multiplier: 1.0,
            wear_multiplier: 1.0,
            impact_threshold: 5000.0,
            max_structural_damage: 0.95,
            allow_partial_repairs: true,
            repair_cost_per_point: 100.0,
        }
    }
}

// ── Sistema danni (DamageSystem) ────────────────────────────────────────────

type CollisionCallback = Box<dyn FnMut(&CollisionEvent)>;
type ComponentCallback = Box<dyn FnMut(DamageType)>;
type DamageChangedCallback = Box<dyn FnMut(f32)>;
type WarningCallback = Box<dyn FnMut(i32)>;

/// Gestore dei danni in stile rFactor2 (port di `DamageSystem`).
pub struct DamageSystem {
    config: DamageConfig,

    zones: [DamageZoneData; DamageZone::COUNT],

    suspension: [SuspensionDamageData; 4],
    engine: EngineDamageData,
    aero: AeroDamageData,
    transmission: TransmissionDamageData,
    brakes: [BrakeDamageData; 4],

    cached_power_multiplier: f32,
    cached_handling_multiplier: f32,
    cached_braking_multiplier: f32,
    cached_downforce_multiplier: f32,
    cached_drag_multiplier: f32,
    cached_cg_shift: Vec3,
    warning_level: i32,
    total_collisions: i32,

    on_collision_occurred: Option<CollisionCallback>,
    on_component_failed: Option<ComponentCallback>,
    on_damage_changed: Option<DamageChangedCallback>,
    on_warning_level_changed: Option<WarningCallback>,
}

impl Default for DamageSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl DamageSystem {
    /// Un sistema danni nello stato iniziale (port di `DamageSystem()`).
    pub fn new() -> Self {
        let mut system = Self {
            config: DamageConfig::default(),
            zones: [DamageZoneData::default(); DamageZone::COUNT],
            suspension: [SuspensionDamageData::default(); 4],
            engine: EngineDamageData::default(),
            aero: AeroDamageData::default(),
            transmission: TransmissionDamageData::default(),
            brakes: [BrakeDamageData::default(); 4],
            cached_power_multiplier: 1.0,
            cached_handling_multiplier: 1.0,
            cached_braking_multiplier: 1.0,
            cached_downforce_multiplier: 1.0,
            cached_drag_multiplier: 1.0,
            cached_cg_shift: Vec3::ZERO,
            warning_level: 0,
            total_collisions: 0,
            on_collision_occurred: None,
            on_component_failed: None,
            on_damage_changed: None,
            on_warning_level_changed: None,
        };
        system.reset();
        system
    }

    /// Imposta la configurazione.
    pub fn set_config(&mut self, config: DamageConfig) {
        self.config = config;
    }

    /// La configurazione corrente.
    pub fn config(&self) -> DamageConfig {
        self.config
    }

    /// Imposta il callback di collisione.
    pub fn set_on_collision_occurred(&mut self, cb: impl FnMut(&CollisionEvent) + 'static) {
        self.on_collision_occurred = Some(Box::new(cb));
    }

    /// Imposta il callback di cedimento componente.
    pub fn set_on_component_failed(&mut self, cb: impl FnMut(DamageType) + 'static) {
        self.on_component_failed = Some(Box::new(cb));
    }

    /// Imposta il callback di variazione danno.
    pub fn set_on_damage_changed(&mut self, cb: impl FnMut(f32) + 'static) {
        self.on_damage_changed = Some(Box::new(cb));
    }

    /// Imposta il callback di variazione del livello di allerta.
    pub fn set_on_warning_level_changed(&mut self, cb: impl FnMut(i32) + 'static) {
        self.on_warning_level_changed = Some(Box::new(cb));
    }

    /// Aggiornamento per passo fisico (port di `update`).
    pub fn update(&mut self, _dt: f32, _speed: f32, _rpm: f32) {
        if !self.config.enabled {
            return;
        }
        self.update_physics_impact();
        self.update_warning_level();
    }

    /// Elabora una collisione (port di `processCollision`).
    pub fn process_collision(&mut self, event: CollisionEvent) {
        if !self.config.enabled {
            return;
        }

        let scaled_energy = event.impact_energy * self.config.global_damage_multiplier;
        if scaled_energy < self.config.impact_threshold {
            return;
        }

        self.distribute_damage_to_zones(event.contact_point, scaled_energy, event.damage_type);

        let force = calculate_impact_force(scaled_energy, 1500.0);
        let scaled_event = CollisionEvent {
            impact_energy: scaled_energy,
            impact_force: force,
            ..event
        };

        self.total_collisions += 1;
        if let Some(cb) = self.on_collision_occurred.as_mut() {
            cb(&scaled_event);
        }
    }

    /// Applica un danno diretto (port di `applyImpactDamage`).
    pub fn apply_impact_damage(
        &mut self,
        energy: f32,
        _direction: Vec3,
        contact_point: Vec3,
        damage_type: DamageType,
    ) {
        if !self.config.enabled {
            return;
        }
        let scaled_energy = energy * self.config.global_damage_multiplier;
        if scaled_energy < self.config.impact_threshold {
            return;
        }
        self.distribute_damage_to_zones(contact_point, scaled_energy, damage_type);
    }

    /// Dati della zona (port di `zoneData`).
    pub fn zone_data(&self, zone: DamageZone) -> DamageZoneData {
        self.zones[zone.index()]
    }

    /// Integrità strutturale della zona (port di `zoneIntegrity`).
    pub fn zone_integrity(&self, zone: DamageZone) -> f32 {
        self.zones[zone.index()].structural
    }

    /// Danno estetico della zona (port di `zoneCosmetic`).
    pub fn zone_cosmetic(&self, zone: DamageZone) -> f32 {
        self.zones[zone.index()].cosmetic
    }

    /// Danno sospensione (port di `suspensionDamage`).
    pub fn suspension_damage(&self, wheel: usize) -> SuspensionDamageData {
        self.suspension[wheel.min(3)]
    }

    /// Danno motore (port di `engineDamage`).
    pub fn engine_damage(&self) -> EngineDamageData {
        self.engine
    }

    /// Danno motore mutabile (per l'usura meccanica).
    pub fn engine_damage_mut(&mut self) -> &mut EngineDamageData {
        &mut self.engine
    }

    /// Danno aerodinamica (port di `aeroDamage`).
    pub fn aero_damage(&self) -> AeroDamageData {
        self.aero
    }

    /// Danno aerodinamica mutabile (per l'usura meccanica).
    pub fn aero_damage_mut(&mut self) -> &mut AeroDamageData {
        &mut self.aero
    }

    /// Danno trasmissione (port di `transmissionDamage`).
    pub fn transmission_damage(&self) -> TransmissionDamageData {
        self.transmission
    }

    /// Danno trasmissione mutabile (per l'usura meccanica).
    pub fn transmission_damage_mut(&mut self) -> &mut TransmissionDamageData {
        &mut self.transmission
    }

    /// Danno sospensione mutabile (per l'usura meccanica).
    pub fn suspension_damage_mut(&mut self, wheel: usize) -> &mut SuspensionDamageData {
        &mut self.suspension[wheel.min(3)]
    }

    /// Danno freni (port di `brakeDamage`).
    pub fn brake_damage(&self, wheel: usize) -> BrakeDamageData {
        self.brakes[wheel.min(3)]
    }

    /// Danno freni mutabile (per l'usura meccanica).
    pub fn brake_damage_mut(&mut self, wheel: usize) -> &mut BrakeDamageData {
        &mut self.brakes[wheel.min(3)]
    }

    /// Danno complessivo (`0` = perfetto, `1` = distrutto), port di `overallDamage`.
    pub fn overall_damage(&self) -> f32 {
        let mut zone_dmg = 0.0;
        for zone in &self.zones {
            zone_dmg += zone.combined_damage();
        }
        zone_dmg /= self.zones.len() as f32;

        let mut component_dmg = 0.0;
        component_dmg += (1.0 - self.engine.health) * 0.3;
        component_dmg += (1.0 - self.aero.downforce_multiplier()) * 0.2;
        component_dmg += (1.0 - self.transmission.health) * 0.2;
        for susp in &self.suspension {
            component_dmg += (1.0 - susp.handling_multiplier()) * 0.075;
        }

        (zone_dmg * 0.6 + component_dmg * 0.4).clamp(0.0, 1.0)
    }

    /// Danno strutturale medio (port di `structuralDamage`).
    pub fn structural_damage(&self) -> f32 {
        let mut total = 0.0;
        for zone in &self.zones {
            total += 1.0 - zone.structural;
        }
        total / self.zones.len() as f32
    }

    /// Danno estetico medio (port di `cosmeticDamage`).
    pub fn cosmetic_damage(&self) -> f32 {
        let mut total = 0.0;
        for zone in &self.zones {
            total += zone.cosmetic;
        }
        total / self.zones.len() as f32
    }

    /// Potenza motore effettiva (port di `powerMultiplier`).
    pub fn power_multiplier(&self) -> f32 {
        self.cached_power_multiplier
    }

    /// Handling effettivo (port di `handlingMultiplier`).
    pub fn handling_multiplier(&self) -> f32 {
        self.cached_handling_multiplier
    }

    /// Frenata effettiva (port di `brakingMultiplier`).
    pub fn braking_multiplier(&self) -> f32 {
        self.cached_braking_multiplier
    }

    /// Deportanza effettiva (port di `downforceMultiplier`).
    pub fn downforce_multiplier(&self) -> f32 {
        self.cached_downforce_multiplier
    }

    /// Variazione di resistenza (port di `dragMultiplier`).
    pub fn drag_multiplier(&self) -> f32 {
        self.cached_drag_multiplier
    }

    /// Spostamento del baricentro (port di `cgShift`).
    pub fn cg_shift(&self) -> Vec3 {
        self.cached_cg_shift
    }

    /// Se il motore è in avaria (port di `isEngineFailed`).
    pub fn is_engine_failed(&self) -> bool {
        self.engine.is_seized
    }

    /// Se la trasmissione è in avaria (port di `isTransmissionFailed`).
    pub fn is_transmission_failed(&self) -> bool {
        self.transmission.is_stuck
    }

    /// Se la sospensione dell'angolo è rotta (port di `isSuspensionBroken`).
    pub fn is_suspension_broken(&self, wheel: usize) -> bool {
        self.suspension[wheel.min(3)].is_broken
    }

    /// Numero totale di collisioni (port di `totalCollisions`).
    pub fn total_collisions(&self) -> i32 {
        self.total_collisions
    }

    /// Livello di allerta corrente.
    pub fn warning_level(&self) -> i32 {
        self.warning_level
    }

    /// Calcola i dati di riparazione (port di `calculateRepairData`).
    pub fn calculate_repair_data(&self) -> RepairData {
        let body_repair_time = self.structural_damage() * 300.0;

        let mut susp_dmg = 0.0;
        for susp in &self.suspension {
            susp_dmg += 1.0 - susp.handling_multiplier();
        }
        susp_dmg /= 4.0;
        let suspension_repair_time = susp_dmg * 180.0;

        let engine_repair_time = if self.engine.is_seized {
            600.0
        } else {
            (1.0 - self.engine.health) * 420.0
        };

        let aero_dmg = (self.aero.front_wing_damage
            + self.aero.rear_wing_damage
            + self.aero.diffuser_damage
            + self.aero.floor_damage)
            / 4.0;
        let aero_repair_time = aero_dmg * 120.0;

        let total_repair_cost = self.overall_damage() * 1000.0 * self.config.repair_cost_per_point;

        let mut needs_replacement = [false; DamageZone::COUNT];
        for (i, zone) in self.zones.iter().enumerate() {
            needs_replacement[i] = zone.structural < 0.2;
        }

        RepairData {
            body_repair_time,
            suspension_repair_time,
            engine_repair_time,
            aero_repair_time,
            total_repair_cost,
            needs_replacement,
        }
    }

    /// Ripara tutto (port di `repairAll`).
    pub fn repair_all(&mut self) {
        for zone in &mut self.zones {
            zone.structural = 1.0;
            zone.cosmetic = 0.0;
            zone.deformation = 0.0;
            zone.impact_energy = 0.0;
            zone.damage_types = DamageType::NONE;
            zone.impact_count = 0;
        }
        self.suspension = [SuspensionDamageData::default(); 4];
        self.engine = EngineDamageData::default();
        self.aero = AeroDamageData::default();
        self.transmission = TransmissionDamageData::default();
        self.brakes = [BrakeDamageData::default(); 4];
        self.total_collisions = 0;
        self.update_physics_impact();
    }

    /// Ripara una frazione del danno (port di `repairPartial`).
    pub fn repair_partial(&mut self, fraction: f32) {
        let fraction = fraction.clamp(0.0, 1.0);

        for zone in &mut self.zones {
            zone.structural = (zone.structural + fraction * (1.0 - zone.structural)).min(1.0);
            zone.cosmetic *= 1.0 - fraction * 0.8;
            zone.deformation *= 1.0 - fraction * 0.7;
        }

        for susp in &mut self.suspension {
            susp.arm_strength = (susp.arm_strength + fraction * (1.0 - susp.arm_strength)).min(1.0);
            susp.geometry = (susp.geometry + fraction * (1.0 - susp.geometry)).min(1.0);
            if susp.is_broken && fraction > 0.5 {
                susp.is_broken = false;
            }
        }

        self.engine.health = (self.engine.health + fraction * (1.0 - self.engine.health)).min(1.0);
        self.engine.overheating *= 1.0 - fraction;
        if self.engine.is_seized && fraction > 0.8 {
            self.engine.is_seized = false;
        }

        self.aero.front_wing_damage *= 1.0 - fraction;
        self.aero.rear_wing_damage *= 1.0 - fraction;
        self.aero.diffuser_damage *= 1.0 - fraction;
        self.aero.floor_damage *= 1.0 - fraction;
        self.aero.radiator_damage *= 1.0 - fraction;

        self.transmission.health =
            (self.transmission.health + fraction * (1.0 - self.transmission.health)).min(1.0);

        for brake in &mut self.brakes {
            brake.pad_wear *= 1.0 - fraction * 0.5;
            brake.disc_damage *= 1.0 - fraction * 0.3;
            brake.fade_level *= 1.0 - fraction;
        }

        self.update_physics_impact();
    }

    /// Ripara un sistema specifico (port di `repairSystem`).
    pub fn repair_system(&mut self, damage_type: DamageType) {
        let repair_amount = 0.8;

        if damage_type.has_flag(DamageType::ENGINE) {
            self.engine.health = (self.engine.health + repair_amount).min(1.0);
            self.engine.overheating *= 0.2;
            self.engine.misfire_rate *= 0.2;
            self.engine.is_seized = false;
        }

        if damage_type.has_flag(DamageType::SUSPENSION) {
            for susp in &mut self.suspension {
                susp.arm_strength = (susp.arm_strength + repair_amount).min(1.0);
                susp.geometry = (susp.geometry + repair_amount * 0.8).min(1.0);
                susp.is_broken = false;
            }
        }

        if damage_type.has_flag(DamageType::AERO) {
            self.aero.front_wing_damage *= 0.2;
            self.aero.rear_wing_damage *= 0.2;
            self.aero.diffuser_damage *= 0.2;
            self.aero.floor_damage *= 0.2;
        }

        if damage_type.has_flag(DamageType::TRANSMISSION) {
            self.transmission.health = (self.transmission.health + repair_amount).min(1.0);
            self.transmission.is_stuck = false;
        }

        if damage_type.has_flag(DamageType::BRAKES) {
            for brake in &mut self.brakes {
                brake.pad_wear *= 0.2;
                brake.disc_damage *= 0.3;
                brake.fade_level = 0.0;
            }
        }

        self.update_physics_impact();
    }

    /// Riporta il sistema allo stato iniziale (port di `reset`).
    pub fn reset(&mut self) {
        for zone in &mut self.zones {
            *zone = DamageZoneData::default();
            zone.structural = 1.0;
        }
        self.suspension = [SuspensionDamageData::default(); 4];
        self.engine = EngineDamageData::default();
        self.aero = AeroDamageData::default();
        self.transmission = TransmissionDamageData::default();
        self.brakes = [BrakeDamageData::default(); 4];
        self.total_collisions = 0;
        self.update_physics_impact();
    }

    /// Ripara una zona (port di `resetZone`).
    pub fn reset_zone(&mut self, zone: DamageZone) {
        let z = &mut self.zones[zone.index()];
        z.structural = 1.0;
        z.cosmetic = 0.0;
        z.deformation = 0.0;
        z.impact_energy = 0.0;
        z.damage_types = DamageType::NONE;
        z.impact_count = 0;
        self.update_physics_impact();
    }

    fn update_physics_impact(&mut self) {
        self.cached_power_multiplier =
            self.engine.effective_power_multiplier() * self.transmission.efficiency_multiplier();

        let mut handling = 1.0_f32;
        for susp in &self.suspension {
            handling = handling.min(susp.handling_multiplier());
        }
        self.cached_handling_multiplier = handling;

        let mut braking = 1.0_f32;
        for brake in &self.brakes {
            braking = braking.min(brake.braking_multiplier());
        }
        self.cached_braking_multiplier = braking;

        self.cached_downforce_multiplier = self.aero.downforce_multiplier();
        self.cached_drag_multiplier = self.aero.drag_multiplier();

        let mut total_shift = Vec3::ZERO;

        let front_damage = (self.zones[0].combined_damage()
            + self.zones[1].combined_damage()
            + self.zones[2].combined_damage())
            / 3.0;
        let rear_damage = (self.zones[3].combined_damage()
            + self.zones[4].combined_damage()
            + self.zones[5].combined_damage())
            / 3.0;
        total_shift.z = front_damage - rear_damage;

        let left_damage = (self.zones[0].combined_damage()
            + self.zones[3].combined_damage()
            + self.zones[6].combined_damage())
            / 3.0;
        let right_damage = (self.zones[2].combined_damage()
            + self.zones[5].combined_damage()
            + self.zones[7].combined_damage())
            / 3.0;
        total_shift.x = left_damage - right_damage;

        self.cached_cg_shift = total_shift;
    }

    fn update_warning_level(&mut self) {
        let overall = self.overall_damage();
        let new_level = if overall > 0.8 || self.engine.is_seized {
            2
        } else if overall > 0.3 || self.engine.health < 0.5 {
            1
        } else {
            0
        };

        if new_level != self.warning_level {
            self.warning_level = new_level;
            if let Some(cb) = self.on_warning_level_changed.as_mut() {
                cb(self.warning_level);
            }
        }
    }

    fn distribute_damage_to_zones(
        &mut self,
        contact_point: Vec3,
        energy: f32,
        damage_type: DamageType,
    ) {
        let front_bias = ((contact_point.z + 2.0) / 4.0).clamp(0.0, 1.0);
        let left_bias = ((contact_point.x + 1.0) / 2.0).clamp(0.0, 1.0);
        let height_bias = (contact_point.y / 1.0).clamp(0.0, 1.0);

        let front_left_weight = front_bias * left_bias * 0.5;
        let front_center_weight = front_bias * (1.0 - (left_bias - 0.5).abs() * 2.0) * 0.6;
        let front_right_weight = front_bias * (1.0 - left_bias) * 0.5;

        let rear_left_weight = (1.0 - front_bias) * left_bias * 0.4;
        let rear_center_weight = (1.0 - front_bias) * (1.0 - (left_bias - 0.5).abs() * 2.0) * 0.5;
        let rear_right_weight = (1.0 - front_bias) * (1.0 - left_bias) * 0.4;

        let left_side_weight = left_bias * (1.0 - front_bias) * 0.3;
        let right_side_weight = (1.0 - left_bias) * (1.0 - front_bias) * 0.3;

        let underbody_weight = (1.0 - height_bias) * 0.3;

        let engine_bay_weight = front_bias * (1.0 - (left_bias - 0.5).abs() * 2.0) * 0.4;

        let cabin_weight =
            (1.0 - (front_bias - 0.5).abs() * 2.0) * (1.0 - (left_bias - 0.5).abs() * 2.0) * 0.3;

        let direction = -contact_point.normalize_or_zero();

        let mut apply_to_zone = |zone: DamageZone, weight: f32| {
            if weight > 0.01 {
                self.zones[zone.index()].apply_impact(energy * weight, direction, damage_type);
            }
        };

        apply_to_zone(DamageZone::FrontLeft, front_left_weight);
        apply_to_zone(DamageZone::FrontCenter, front_center_weight);
        apply_to_zone(DamageZone::FrontRight, front_right_weight);
        apply_to_zone(DamageZone::RearLeft, rear_left_weight);
        apply_to_zone(DamageZone::RearCenter, rear_center_weight);
        apply_to_zone(DamageZone::RearRight, rear_right_weight);
        apply_to_zone(DamageZone::LeftSide, left_side_weight);
        apply_to_zone(DamageZone::RightSide, right_side_weight);
        apply_to_zone(DamageZone::EngineBay, engine_bay_weight);
        apply_to_zone(DamageZone::Cabin, cabin_weight);
        apply_to_zone(DamageZone::Underbody, underbody_weight);

        let normalized_energy = (energy / 50000.0).min(1.0) * self.config.physics_damage_multiplier;

        if front_left_weight + front_right_weight > 0.2 {
            let front_dmg = normalized_energy * (front_left_weight + front_right_weight);
            self.suspension[0].arm_strength =
                (self.suspension[0].arm_strength - front_dmg * 0.3).max(0.0);
            self.suspension[0].geometry = (self.suspension[0].geometry - front_dmg * 0.2).max(0.0);
            self.suspension[1].arm_strength =
                (self.suspension[1].arm_strength - front_dmg * 0.3).max(0.0);
            self.suspension[1].geometry = (self.suspension[1].geometry - front_dmg * 0.2).max(0.0);
            self.aero.front_wing_damage = (self.aero.front_wing_damage + front_dmg * 0.4).min(1.0);
        }

        if rear_left_weight + rear_right_weight > 0.2 {
            let rear_dmg = normalized_energy * (rear_left_weight + rear_right_weight);
            self.suspension[2].arm_strength =
                (self.suspension[2].arm_strength - rear_dmg * 0.3).max(0.0);
            self.suspension[3].arm_strength =
                (self.suspension[3].arm_strength - rear_dmg * 0.3).max(0.0);
            self.aero.rear_wing_damage = (self.aero.rear_wing_damage + rear_dmg * 0.3).min(1.0);
        }

        if engine_bay_weight > 0.1 {
            let eng_dmg = normalized_energy * engine_bay_weight;
            self.engine.health = (self.engine.health - eng_dmg * 0.3).max(0.0);
            self.engine.overheating = (self.engine.overheating + eng_dmg * 0.2).min(1.0);
            self.aero.radiator_damage = (self.aero.radiator_damage + eng_dmg * 0.25).min(1.0);
        }

        if underbody_weight > 0.1 {
            let floor_dmg = normalized_energy * underbody_weight;
            self.aero.floor_damage = (self.aero.floor_damage + floor_dmg * 0.35).min(1.0);
            self.aero.diffuser_damage = (self.aero.diffuser_damage + floor_dmg * 0.25).min(1.0);
        }

        let engine_seized = self.engine.health <= 0.05;
        if engine_seized {
            self.engine.is_seized = true;
        }
        let mut broken_wheel = None;
        for i in 0..4 {
            if self.suspension[i].arm_strength <= 0.1 {
                self.suspension[i].is_broken = true;
                if broken_wheel.is_none() {
                    broken_wheel = Some(i);
                }
            }
        }

        if engine_seized {
            self.emit_component_failed(DamageType::ENGINE);
        }
        if broken_wheel.is_some() {
            self.emit_component_failed(DamageType::SUSPENSION);
        }

        self.emit_damage_changed();
    }

    fn emit_component_failed(&mut self, damage_type: DamageType) {
        if let Some(cb) = self.on_component_failed.as_mut() {
            cb(damage_type);
        }
    }

    fn emit_damage_changed(&mut self) {
        let overall = self.overall_damage();
        if let Some(cb) = self.on_damage_changed.as_mut() {
            cb(overall);
        }
    }
}

/// Forza di picco da energia d'impatto (port di `calculateImpactForce`).
pub fn calculate_impact_force(energy: f32, _mass: f32) -> f32 {
    let effective_stiffness = 500000.0;
    (2.0 * energy * effective_stiffness).sqrt()
}

// ── Ponte rFactor2 (Rf2DamageModel) ─────────────────────────────────────────

/// Parametri del ponte rF2 (port di `Rf2DamageParams`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rf2DamageParams {
    /// Impulso "morbido".
    pub soft_impulse: f32,
    /// Impulso "duro".
    pub hard_impulse: f32,
    /// Scala carrozzeria.
    pub body_scale: f32,
    /// Scala sospensioni.
    pub suspension_scale: f32,
    /// Scala aerodinamica.
    pub aero_scale: f32,
    /// Se abilitato.
    pub enabled: bool,
}

impl Default for Rf2DamageParams {
    fn default() -> Self {
        Self {
            soft_impulse: 500.0,
            hard_impulse: 5000.0,
            body_scale: 1.0,
            suspension_scale: 1.0,
            aero_scale: 1.0,
            enabled: true,
        }
    }
}

/// Impulso da velocità relativa e massa (port di `impulseFromImpact`).
pub fn impulse_from_impact(relative_speed_ms: f32, mass_kg: f32) -> f32 {
    let v = relative_speed_ms.max(0.0);
    let m = mass_kg.max(1.0);
    m * v * 0.35
}

/// Applica un impulso rF2 (port di `applyRf2Impulse`).
pub fn apply_rf2_impulse(
    dmg: &mut DamageSystem,
    params: &Rf2DamageParams,
    impulse: f32,
    contact_point: Vec3,
    contact_normal: Vec3,
) {
    if !params.enabled || impulse < 1.0 {
        return;
    }
    let dir = if contact_normal.length() > 1e-4 {
        contact_normal.normalize()
    } else {
        Vec3::new(0.0, 0.0, 1.0)
    };
    let mut energy = impulse * impulse * 0.001 * params.body_scale;
    energy = energy.clamp(0.0, 1.0e7);
    let mut damage_type = DamageType::BODY_PANEL;
    if impulse > params.hard_impulse {
        damage_type = damage_type | DamageType::SUSPENSION | DamageType::AERO;
    }
    dmg.apply_impact_damage(energy, dir, contact_point, damage_type);
}

// ── Usura meccanica (MechanicalDamage) ──────────────────────────────────────

/// Telemetria per l'usura meccanica (port di `MechTelemetry`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MechTelemetry {
    /// Velocità, m/s.
    pub speed_ms: f32,
    /// Regime motore, rpm.
    pub rpm: f32,
    /// Regime massimo, rpm.
    pub max_rpm: f32,
    /// Acceleratore, `0..=1`.
    pub throttle: f32,
    /// Freno, `0..=1`.
    pub brake: f32,
    /// Frizione (`1` = innestata).
    pub clutch: f32,
    /// Marcia.
    pub gear: i32,
    /// Se è in corso un cambio marcia.
    pub is_shifting: bool,
    /// Temperatura motore, °C.
    pub engine_temp_c: f32,
    /// Temperatura olio, °C.
    pub oil_temp_c: f32,
    /// Pressione olio, bar.
    pub oil_pressure_bar: f32,
    /// Temperatura refrigerante, °C.
    pub coolant_temp_c: f32,
    /// Temperatura freni per angolo, °C.
    pub brake_temp_c: [f32; 4],
    /// Corsa sospensioni (`0..=1` in compressione).
    pub suspension_travel: [f32; 4],
    /// Velocità verticale sospensioni, m/s.
    pub suspension_vel: [f32; 4],
    /// G verticale.
    pub vertical_g: f32,
    /// Aggressività dei cordoli per angolo, `0..=1`.
    pub curb_load: [f32; 4],
    /// Se il motore è in funzione.
    pub engine_running: bool,
}

impl Default for MechTelemetry {
    fn default() -> Self {
        Self {
            speed_ms: 0.0,
            rpm: 0.0,
            max_rpm: 7500.0,
            throttle: 0.0,
            brake: 0.0,
            clutch: 1.0,
            gear: 0,
            is_shifting: false,
            engine_temp_c: 90.0,
            oil_temp_c: 100.0,
            oil_pressure_bar: 4.0,
            coolant_temp_c: 90.0,
            brake_temp_c: [300.0; 4],
            suspension_travel: [0.0; 4],
            suspension_vel: [0.0; 4],
            vertical_g: 1.0,
            curb_load: [0.0; 4],
            engine_running: true,
        }
    }
}

/// Configurazione dell'usura meccanica (port di `MechWearConfig`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MechWearConfig {
    /// Inizio fuorigiri (frazione del max rpm).
    pub over_rev_start: f32,
    /// Grippaggio da fuorigiri.
    pub over_rev_seize: f32,
    /// Danno per secondo in fuorigiri.
    pub over_rev_damage_per_sec: f32,
    /// Inizio surriscaldamento, °C.
    pub overheat_start_c: f32,
    /// Surriscaldamento critico, °C.
    pub overheat_critical_c: f32,
    /// Danno per secondo da surriscaldamento.
    pub overheat_damage_per_sec: f32,
    /// Pressione olio bassa, bar.
    pub low_oil_pressure_bar: f32,
    /// Danno per secondo da pressione olio bassa.
    pub low_oil_damage_per_sec: f32,
    /// Salute sotto la quale inizia il misfire.
    pub misfire_from_health: f32,
    /// Danno per secondo da slittamento frizione.
    pub clutch_slip_damage_per_sec: f32,
    /// Danno per cambio in potenza.
    pub power_shift_damage: f32,
    /// Velocità di abuso del differenziale, m/s.
    pub diff_abuse_speed_ms: f32,
    /// Corsa di fondocorsa.
    pub bottom_travel: f32,
    /// Danno per evento di fondocorsa.
    pub bottom_damage: f32,
    /// Scala danno da cordoli.
    pub curb_damage_scale: f32,
    /// Soglia di rottura del braccio.
    pub arm_break_threshold: f32,
    /// Usura pastiglie per secondo a fondo.
    pub pad_wear_per_sec_at_full: f32,
    /// Inizio fade, °C.
    pub fade_start_c: f32,
    /// Fade critico, °C.
    pub fade_critical_c: f32,
    /// Danno disco per secondo sopra la critica.
    pub disc_damage_at_critical: f32,
    /// Accoppiamento impatto→motore.
    pub impact_engine_coupling: f32,
    /// Accoppiamento impatto→sospensioni.
    pub impact_susp_coupling: f32,
    /// Accoppiamento impatto→aerodinamica.
    pub impact_aero_coupling: f32,
    /// Accoppiamento impatto→trasmissione.
    pub impact_trans_coupling: f32,
    /// Energia di impatto frontale che può grippare.
    pub seize_energy: f32,
    /// Scala globale dell'usura.
    pub global_wear_scale: f32,
}

impl Default for MechWearConfig {
    fn default() -> Self {
        Self {
            over_rev_start: 1.02,
            over_rev_seize: 1.12,
            over_rev_damage_per_sec: 0.08,
            overheat_start_c: 110.0,
            overheat_critical_c: 130.0,
            overheat_damage_per_sec: 0.04,
            low_oil_pressure_bar: 1.5,
            low_oil_damage_per_sec: 0.06,
            misfire_from_health: 0.55,
            clutch_slip_damage_per_sec: 0.05,
            power_shift_damage: 0.02,
            diff_abuse_speed_ms: 40.0,
            bottom_travel: 0.92,
            bottom_damage: 0.015,
            curb_damage_scale: 0.01,
            arm_break_threshold: 0.15,
            pad_wear_per_sec_at_full: 0.0008,
            fade_start_c: 550.0,
            fade_critical_c: 750.0,
            disc_damage_at_critical: 0.02,
            impact_engine_coupling: 0.25,
            impact_susp_coupling: 0.45,
            impact_aero_coupling: 0.35,
            impact_trans_coupling: 0.15,
            seize_energy: 80000.0,
            global_wear_scale: 1.0,
        }
    }
}

/// Applica l'usura meccanica continua per un frame (port di `applyMechanicalWear`).
pub fn apply_mechanical_wear(
    dmg: &mut DamageSystem,
    dt: f32,
    t: &MechTelemetry,
    cfg: &MechWearConfig,
) {
    if !dmg.config().enabled || dt <= 0.0 {
        return;
    }

    let s = cfg.global_wear_scale * dmg.config().wear_multiplier;

    let mut engine_failed = false;
    if t.engine_running && !dmg.engine.is_seized {
        let eng = &mut dmg.engine;
        let rpm_ratio = if t.max_rpm > 1.0 {
            t.rpm / t.max_rpm
        } else {
            0.0
        };

        if rpm_ratio > cfg.over_rev_start {
            let severity = ((rpm_ratio - cfg.over_rev_start)
                / (cfg.over_rev_seize - cfg.over_rev_start).max(0.01))
            .clamp(0.0, 1.0);
            eng.health = (eng.health - cfg.over_rev_damage_per_sec * severity * dt * s).max(0.0);
            eng.power_loss = (eng.power_loss + severity * 0.02 * dt * s).min(0.9);
            if rpm_ratio >= cfg.over_rev_seize && severity > 0.85 {
                eng.is_seized = true;
                eng.health = 0.0;
                engine_failed = true;
            }
        }

        if t.coolant_temp_c > cfg.overheat_start_c || t.engine_temp_c > cfg.overheat_start_c {
            let peak = t.coolant_temp_c.max(t.engine_temp_c);
            let sev = ((peak - cfg.overheat_start_c)
                / (cfg.overheat_critical_c - cfg.overheat_start_c).max(1.0))
            .clamp(0.0, 1.0);
            eng.overheating = (eng.overheating + sev * 0.15 * dt * s).min(1.0);
            eng.health = (eng.health - cfg.overheat_damage_per_sec * sev * dt * s).max(0.0);
            eng.coolant_leak = (eng.coolant_leak + sev * 0.01 * dt * s).min(1.0);
        } else {
            eng.overheating = (eng.overheating - 0.05 * dt).max(0.0);
        }

        if t.oil_pressure_bar < cfg.low_oil_pressure_bar && t.rpm > 1500.0 {
            let sev = 1.0 - (t.oil_pressure_bar / cfg.low_oil_pressure_bar).clamp(0.0, 1.0);
            eng.oil_pressure_loss = (eng.oil_pressure_loss + sev * 0.1 * dt * s).min(1.0);
            eng.health = (eng.health - cfg.low_oil_damage_per_sec * sev * dt * s).max(0.0);
        }

        if eng.health < cfg.misfire_from_health {
            eng.misfire_rate = ((cfg.misfire_from_health - eng.health) * 1.5).min(1.0);
        }
    }
    if engine_failed {
        dmg.emit_component_failed(DamageType::ENGINE);
    }

    let mut trans_failed = false;
    if !dmg.transmission.is_stuck {
        let trans = &mut dmg.transmission;
        if t.clutch < 0.85 && t.throttle > 0.3 && t.rpm > 2000.0 {
            let slip = (1.0 - t.clutch) * t.throttle;
            trans.clutch_damage =
                (trans.clutch_damage + cfg.clutch_slip_damage_per_sec * slip * dt * s).min(1.0);
            trans.health = (trans.health - 0.01 * slip * dt * s).max(0.2);
        }
        if t.is_shifting && t.throttle > 0.7 {
            let gi = t.gear.clamp(0, 7) as usize;
            trans.gear_damage[gi] = (trans.gear_damage[gi] + cfg.power_shift_damage * s).min(1.0);
        }
        if trans.clutch_damage > 0.95 || trans.health < 0.25 {
            trans.is_stuck = true;
            trans_failed = true;
        }
    }
    if trans_failed {
        dmg.emit_component_failed(DamageType::TRANSMISSION);
    }

    for i in 0..4 {
        let mut broke = false;
        {
            let sus = &mut dmg.suspension[i];
            if !sus.is_broken {
                if t.suspension_travel[i] >= cfg.bottom_travel {
                    sus.spring_damage = (sus.spring_damage + cfg.bottom_damage * s).min(1.0);
                    sus.damping_loss = (sus.damping_loss + cfg.bottom_damage * 0.5 * s).min(1.0);
                    sus.geometry = (sus.geometry - cfg.bottom_damage * 0.3 * s).max(0.0);
                }
                if t.curb_load[i] > 0.05 {
                    let c = t.curb_load[i] * cfg.curb_damage_scale * dt * s;
                    sus.arm_strength = (sus.arm_strength - c * 2.0).max(0.0);
                    sus.toe_deviation += (if i % 2 == 0 { -1.0 } else { 1.0 }) * c * 8.0;
                    sus.camber_deviation += c * 5.0;
                    sus.geometry = (sus.geometry - c).max(0.0);
                }
                if sus.arm_strength < cfg.arm_break_threshold || sus.geometry < 0.1 {
                    sus.is_broken = true;
                    broke = true;
                }
            }
        }
        if broke {
            dmg.emit_component_failed(DamageType::SUSPENSION);
        }
    }

    for i in 0..4 {
        let br = &mut dmg.brakes[i];
        br.temperature = t.brake_temp_c[i];

        if t.brake > 0.05 && t.speed_ms > 5.0 {
            br.pad_wear = (br.pad_wear + cfg.pad_wear_per_sec_at_full * t.brake * dt * s).min(1.0);
        }
        if br.temperature > cfg.fade_start_c {
            let sev = ((br.temperature - cfg.fade_start_c)
                / (cfg.fade_critical_c - cfg.fade_start_c).max(1.0))
            .clamp(0.0, 1.0);
            br.fade_level = sev.min(1.0);
            br.is_faded = sev > 0.6;
            if br.temperature > cfg.fade_critical_c {
                br.disc_damage = (br.disc_damage + cfg.disc_damage_at_critical * dt * s).min(1.0);
            }
        } else {
            br.fade_level = (br.fade_level - 0.2 * dt).max(0.0);
            br.is_faded = false;
        }
    }

    dmg.update(0.0, t.speed_ms, t.rpm);
}

/// Mappa un impatto nel mondo sul sistema danni (port di `applyMechanicalImpact`).
#[allow(clippy::too_many_arguments)]
pub fn apply_mechanical_impact(
    dmg: &mut DamageSystem,
    energy: f32,
    local_x: f32,
    local_y: f32,
    local_z: f32,
    normal_x: f32,
    normal_y: f32,
    normal_z: f32,
    cfg: &MechWearConfig,
) {
    if !dmg.config().enabled {
        return;
    }

    let primary_zone = if local_z > 0.6 {
        DamageZone::FrontCenter
    } else if local_z < -0.6 {
        DamageZone::RearCenter
    } else if local_x > 0.4 {
        DamageZone::RightSide
    } else if local_x < -0.4 {
        DamageZone::LeftSide
    } else {
        DamageZone::Underbody
    };

    let mut damage_type = DamageType::BODY_PANEL;
    if local_y < 0.3 {
        damage_type = damage_type | DamageType::SUSPENSION;
    }
    if local_z > 0.5 && local_y > 0.4 {
        damage_type = damage_type | DamageType::AERO;
    }
    if local_z > 0.4 && local_x.abs() < 0.5 {
        damage_type = damage_type | DamageType::ENGINE;
    }

    let event = CollisionEvent {
        contact_point: Vec3::new(local_x, local_y, local_z),
        contact_normal: Vec3::new(normal_x, normal_y, normal_z),
        impact_energy: energy,
        impact_force: energy,
        primary_zone,
        damage_type,
        timestamp: 0.0,
        ..Default::default()
    };

    dmg.process_collision(event);

    let n = (energy / 50000.0).min(1.0) * dmg.config().physics_damage_multiplier;

    let mut engine_failed = false;
    if local_z > 0.3 {
        let eng = &mut dmg.engine;
        eng.health = (eng.health - n * cfg.impact_engine_coupling).max(0.0);
        eng.power_loss = (eng.power_loss + n * cfg.impact_engine_coupling * 0.5).min(0.9);
        dmg.aero.front_wing_damage =
            (dmg.aero.front_wing_damage + n * cfg.impact_aero_coupling).min(1.0);
        dmg.aero.radiator_damage = (dmg.aero.radiator_damage + n * 0.2).min(1.0);
        if energy > cfg.seize_energy && local_z > 0.8 {
            dmg.engine.is_seized = true;
            dmg.engine.health = 0.0;
            engine_failed = true;
        }
    }
    if local_z < -0.3 {
        dmg.aero.rear_wing_damage =
            (dmg.aero.rear_wing_damage + n * cfg.impact_aero_coupling).min(1.0);
    }
    if local_y < 0.25 {
        dmg.aero.floor_damage = (dmg.aero.floor_damage + n * 0.4).min(1.0);
    }
    if engine_failed {
        dmg.emit_component_failed(DamageType::ENGINE);
    }

    let wheel = if local_z > 0.0 {
        if local_x >= 0.0 {
            1
        } else {
            0
        }
    } else if local_x >= 0.0 {
        3
    } else {
        2
    };
    let sus = &mut dmg.suspension[wheel];
    sus.arm_strength = (sus.arm_strength - n * cfg.impact_susp_coupling).max(0.0);
    sus.geometry = (sus.geometry - n * cfg.impact_susp_coupling * 0.7).max(0.0);
    if sus.arm_strength < 0.15 {
        sus.is_broken = true;
    }

    dmg.transmission.health = (dmg.transmission.health - n * cfg.impact_trans_coupling).max(0.2);

    dmg.update(0.0, 0.0, 0.0);
}

/// Danno da contatto ai box (port di `applyPitContactDamage`).
pub fn apply_pit_contact_damage(
    dmg: &mut DamageSystem,
    impulse_ns: f32,
    mass_kg: f32,
    local_x: f32,
    local_z: f32,
    cfg: &MechWearConfig,
) {
    let energy = (impulse_ns * impulse_ns) / (2.0 * mass_kg).max(1.0);
    apply_mechanical_impact(
        dmg, energy, local_x, 0.4, local_z, -local_x, 0.0, -local_z, cfg,
    );
}

/// Moltiplicatori aggregati per il simulatore (port di `MechEffectMultipliers`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MechEffectMultipliers {
    /// Potenza.
    pub power: f32,
    /// Handling.
    pub handling: f32,
    /// Frenata.
    pub braking: f32,
    /// Deportanza.
    pub downforce: f32,
    /// Resistenza.
    pub drag: f32,
    /// Consumo carburante.
    pub fuel_use: f32,
    /// Motore morto.
    pub engine_dead: bool,
    /// Trasmissione bloccata.
    pub trans_stuck: bool,
}

/// Campiona i moltiplicatori (port di `sampleMechanicalEffects`).
pub fn sample_mechanical_effects(dmg: &DamageSystem) -> MechEffectMultipliers {
    MechEffectMultipliers {
        power: dmg.power_multiplier(),
        handling: dmg.handling_multiplier(),
        braking: dmg.braking_multiplier(),
        downforce: dmg.downforce_multiplier(),
        drag: dmg.drag_multiplier(),
        fuel_use: 1.0 + dmg.engine_damage().fuel_consumption_increase(),
        engine_dead: dmg.is_engine_failed(),
        trans_stuck: dmg.is_transmission_failed(),
    }
}

// ── Telemetria danni (DamageTelemetry) ──────────────────────────────────────

/// Snapshot piatto dello stato danni (port di `DamageTelemSample`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DamageTelemSample {
    /// Danno frazionario AC-style: F R L R complessivo.
    pub car_damage: [f32; 5],
    /// Danno complessivo, `0..=1`.
    pub overall: f32,
    /// Danno strutturale.
    pub structural: f32,
    /// Danno estetico.
    pub cosmetic: f32,
    /// Danno ala anteriore.
    pub aero_front: f32,
    /// Danno ala posteriore.
    pub aero_rear: f32,
    /// Danno al fondo.
    pub aero_floor: f32,
    /// Salute motore (`1` = sana).
    pub engine_health: f32,
    /// Moltiplicatore di potenza.
    pub power_mult: f32,
    /// Moltiplicatore di resistenza.
    pub drag_mult: f32,
    /// Moltiplicatore di deportanza.
    pub downforce_mult: f32,
    /// Moltiplicatore di frenata.
    pub brake_mult: f32,
    /// Moltiplicatore di handling.
    pub handling_mult: f32,
    /// Integrità sospensioni (FL FR RL RR).
    pub susp_integrity: [f32; 4],
    /// Numero di collisioni.
    pub collision_count: i32,
    /// Livello di allerta (`0` ok, `1` warn, `2` critico).
    pub warning_level: i32,
    /// Motore grippato.
    pub engine_seized: bool,
    /// Trasmissione bloccata.
    pub transmission_stuck: bool,
}

impl Default for DamageTelemSample {
    fn default() -> Self {
        Self {
            car_damage: [0.0; 5],
            overall: 0.0,
            structural: 0.0,
            cosmetic: 0.0,
            aero_front: 0.0,
            aero_rear: 0.0,
            aero_floor: 0.0,
            engine_health: 1.0,
            power_mult: 1.0,
            drag_mult: 1.0,
            downforce_mult: 1.0,
            brake_mult: 1.0,
            handling_mult: 1.0,
            susp_integrity: [1.0; 4],
            collision_count: 0,
            warning_level: 0,
            engine_seized: false,
            transmission_stuck: false,
        }
    }
}

/// Campiona lo stato danni per la telemetria (port di `sampleDamage`).
pub fn sample_damage(d: &DamageSystem) -> DamageTelemSample {
    let overall = d.overall_damage();
    let aero = d.aero_damage();

    let mut susp_integrity = [1.0; 4];
    for (i, value) in susp_integrity.iter_mut().enumerate() {
        *value = d.suspension_damage(i).handling_multiplier();
    }

    let fl = d.zone_data(DamageZone::FrontLeft).combined_damage();
    let fc = d.zone_data(DamageZone::FrontCenter).combined_damage();
    let fr = d.zone_data(DamageZone::FrontRight).combined_damage();
    let rl = d.zone_data(DamageZone::RearLeft).combined_damage();
    let rc = d.zone_data(DamageZone::RearCenter).combined_damage();
    let rr = d.zone_data(DamageZone::RearRight).combined_damage();
    let ls = d.zone_data(DamageZone::LeftSide).combined_damage();
    let rs = d.zone_data(DamageZone::RightSide).combined_damage();

    let engine_health = d.engine_damage().health;
    let engine_seized = d.is_engine_failed();

    let warning_level = if engine_seized || overall > 0.8 {
        2
    } else if overall > 0.3 || engine_health < 0.5 {
        1
    } else {
        0
    };

    DamageTelemSample {
        car_damage: [
            ((fl + fc + fr) / 3.0).clamp(0.0, 1.0),
            ((rl + rc + rr) / 3.0).clamp(0.0, 1.0),
            ((fl + rl + ls) / 3.0).clamp(0.0, 1.0),
            ((fr + rr + rs) / 3.0).clamp(0.0, 1.0),
            overall.clamp(0.0, 1.0),
        ],
        overall,
        structural: d.structural_damage(),
        cosmetic: d.cosmetic_damage(),
        aero_front: aero.front_wing_damage,
        aero_rear: aero.rear_wing_damage,
        aero_floor: aero.floor_damage,
        engine_health,
        power_mult: d.power_multiplier(),
        drag_mult: d.drag_multiplier(),
        downforce_mult: d.downforce_multiplier(),
        brake_mult: d.braking_multiplier(),
        handling_mult: d.handling_multiplier(),
        susp_integrity,
        collision_count: d.total_collisions(),
        warning_level,
        engine_seized,
        transmission_stuck: d.is_transmission_failed(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_car_has_no_damage() {
        let dmg = DamageSystem::new();
        assert_eq!(dmg.overall_damage(), 0.0);
        assert_eq!(dmg.power_multiplier(), 1.0);
        assert_eq!(dmg.handling_multiplier(), 1.0);
        assert_eq!(dmg.total_collisions(), 0);
        for zone in [DamageZone::FrontLeft, DamageZone::Underbody] {
            assert_eq!(dmg.zone_integrity(zone), 1.0);
            assert_eq!(dmg.zone_data(zone).combined_damage(), 0.0);
        }
    }

    #[test]
    fn zone_combined_damage_endpoints() {
        let mut zone = DamageZoneData {
            structural: 1.0,
            ..Default::default()
        };
        assert_eq!(zone.combined_damage(), 0.0);
        for _ in 0..4 {
            zone.apply_impact(50000.0, Vec3::NEG_Z, DamageType::BODY_PANEL);
        }
        assert!((zone.combined_damage() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn impact_below_threshold_is_ignored() {
        let mut dmg = DamageSystem::new();
        dmg.process_collision(CollisionEvent {
            contact_point: Vec3::new(0.0, 0.5, 2.0),
            impact_energy: 100.0,
            damage_type: DamageType::BODY_PANEL,
            ..Default::default()
        });
        assert_eq!(dmg.total_collisions(), 0);
        assert_eq!(dmg.overall_damage(), 0.0);
    }

    #[test]
    fn front_impact_damages_front_zones_and_engine() {
        let mut dmg = DamageSystem::new();
        dmg.process_collision(CollisionEvent {
            contact_point: Vec3::new(0.0, 0.4, 2.0),
            impact_energy: 40000.0,
            damage_type: DamageType::BODY_PANEL | DamageType::ENGINE,
            ..Default::default()
        });
        assert_eq!(dmg.total_collisions(), 1);
        assert!(dmg.zone_data(DamageZone::FrontCenter).structural < 1.0);
        assert!(
            dmg.zone_data(DamageZone::RearCenter).structural
                > dmg.zone_data(DamageZone::FrontCenter).structural
        );
        assert!(dmg.engine_damage().health < 1.0);
        assert!(dmg.overall_damage() > 0.0);
    }

    #[test]
    fn huge_front_impact_seizes_engine() {
        let mut dmg = DamageSystem::new();
        dmg.set_config(DamageConfig {
            impact_threshold: 1.0,
            ..Default::default()
        });
        for _ in 0..20 {
            dmg.process_collision(CollisionEvent {
                contact_point: Vec3::new(0.0, 0.4, 3.0),
                impact_energy: 50000.0,
                damage_type: DamageType::ENGINE,
                ..Default::default()
            });
        }
        dmg.update(0.0, 0.0, 0.0);
        assert!(dmg.engine_damage().health < 0.2);
        assert!(dmg.is_engine_failed());
        assert_eq!(dmg.power_multiplier(), 0.0);
    }

    #[test]
    fn cosmetic_and_structural_are_distinct() {
        let mut dmg = DamageSystem::new();
        dmg.set_config(DamageConfig {
            impact_threshold: 1.0,
            ..Default::default()
        });
        dmg.process_collision(CollisionEvent {
            contact_point: Vec3::new(0.0, 0.5, 2.5),
            impact_energy: 20000.0,
            damage_type: DamageType::BODY_PANEL,
            ..Default::default()
        });
        assert!(dmg.structural_damage() > 0.0);
        assert!(dmg.cosmetic_damage() > 0.0);
    }

    #[test]
    fn repair_all_restores_perfect_condition() {
        let mut dmg = DamageSystem::new();
        dmg.set_config(DamageConfig {
            impact_threshold: 1.0,
            ..Default::default()
        });
        dmg.process_collision(CollisionEvent {
            contact_point: Vec3::new(0.0, 0.4, 2.0),
            impact_energy: 50000.0,
            damage_type: DamageType::ENGINE,
            ..Default::default()
        });
        assert!(dmg.overall_damage() > 0.0);
        dmg.repair_all();
        assert_eq!(dmg.overall_damage(), 0.0);
        assert_eq!(dmg.total_collisions(), 0);
        assert_eq!(dmg.power_multiplier(), 1.0);
    }

    #[test]
    fn repair_partial_reduces_damage() {
        let mut dmg = DamageSystem::new();
        dmg.set_config(DamageConfig {
            impact_threshold: 1.0,
            ..Default::default()
        });
        dmg.process_collision(CollisionEvent {
            contact_point: Vec3::new(-0.9, 0.5, 1.0),
            impact_energy: 50000.0,
            damage_type: DamageType::BODY_PANEL,
            ..Default::default()
        });
        let before = dmg.overall_damage();
        dmg.repair_partial(0.5);
        assert!(dmg.overall_damage() < before);
    }

    #[test]
    fn repair_system_engine_clears_seize() {
        let mut dmg = DamageSystem::new();
        dmg.set_config(DamageConfig {
            impact_threshold: 1.0,
            ..Default::default()
        });
        for _ in 0..20 {
            dmg.process_collision(CollisionEvent {
                contact_point: Vec3::new(0.0, 0.4, 3.0),
                impact_energy: 50000.0,
                damage_type: DamageType::ENGINE,
                ..Default::default()
            });
        }
        assert!(dmg.is_engine_failed());
        dmg.repair_system(DamageType::ENGINE);
        assert!(!dmg.is_engine_failed());
        assert!(dmg.engine_damage().health >= 0.8);
    }

    #[test]
    fn cg_shift_points_away_from_damage() {
        let mut dmg = DamageSystem::new();
        dmg.set_config(DamageConfig {
            impact_threshold: 1.0,
            ..Default::default()
        });
        dmg.process_collision(CollisionEvent {
            contact_point: Vec3::new(0.0, 0.5, 2.5),
            impact_energy: 50000.0,
            damage_type: DamageType::BODY_PANEL,
            ..Default::default()
        });
        dmg.update(0.0, 0.0, 0.0);
        assert!(dmg.cg_shift().z > 0.0);
    }

    #[test]
    fn rf2_impulse_maps_energy() {
        let mut dmg = DamageSystem::new();
        dmg.set_config(DamageConfig {
            impact_threshold: 1.0,
            ..Default::default()
        });
        let impulse = impulse_from_impact(30.0, 1500.0);
        assert!(impulse > 0.0);
        apply_rf2_impulse(
            &mut dmg,
            &Rf2DamageParams::default(),
            impulse,
            Vec3::new(0.0, 0.5, 2.0),
            Vec3::new(0.0, 0.0, -1.0),
        );
        assert!(dmg.overall_damage() > 0.0);
    }

    #[test]
    fn mechanical_wear_over_rev_seizes_engine() {
        let mut dmg = DamageSystem::new();
        let t = MechTelemetry {
            engine_running: true,
            rpm: 9000.0,
            max_rpm: 7500.0,
            ..Default::default()
        };
        for _ in 0..5000 {
            apply_mechanical_wear(&mut dmg, 0.016, &t, &MechWearConfig::default());
        }
        assert!(dmg.is_engine_failed());
    }

    #[test]
    fn mechanical_wear_brakes_fade_and_wear() {
        let mut dmg = DamageSystem::new();
        let t = MechTelemetry {
            brake: 1.0,
            speed_ms: 60.0,
            brake_temp_c: [800.0; 4],
            ..Default::default()
        };
        for _ in 0..2000 {
            apply_mechanical_wear(&mut dmg, 0.016, &t, &MechWearConfig::default());
        }
        let brake = dmg.brake_damage(0);
        assert!(brake.pad_wear > 0.0);
        assert!(brake.disc_damage > 0.0);
    }

    #[test]
    fn mechanical_impact_damages_engine_and_suspension() {
        let mut dmg = DamageSystem::new();
        dmg.set_config(DamageConfig {
            impact_threshold: 1.0,
            ..Default::default()
        });
        apply_mechanical_impact(
            &mut dmg,
            60000.0,
            0.3,
            0.4,
            1.0,
            -0.3,
            0.0,
            -1.0,
            &MechWearConfig::default(),
        );
        assert!(dmg.engine_damage().health < 1.0);
        assert!(dmg.aero_damage().front_wing_damage > 0.0);
    }

    #[test]
    fn callbacks_fire() {
        use std::cell::Cell;
        use std::rc::Rc;

        let mutated = Rc::new(Cell::new(0.0_f32));
        let mut dmg = DamageSystem::new();
        dmg.set_config(DamageConfig {
            impact_threshold: 1.0,
            ..Default::default()
        });
        let m = Rc::clone(&mutated);
        dmg.set_on_damage_changed(move |d| m.set(d));

        dmg.process_collision(CollisionEvent {
            contact_point: Vec3::new(0.0, 0.5, 2.5),
            impact_energy: 40000.0,
            damage_type: DamageType::BODY_PANEL,
            ..Default::default()
        });
        assert!(mutated.get() > 0.0);
    }

    #[test]
    fn telemetry_layout_matches_reference() {
        let mut dmg = DamageSystem::new();
        dmg.set_config(DamageConfig {
            impact_threshold: 1.0,
            ..Default::default()
        });
        assert_eq!(sample_damage(&dmg).warning_level, 0);
        dmg.process_collision(CollisionEvent {
            contact_point: Vec3::new(0.0, 0.4, 2.5),
            impact_energy: 50000.0,
            damage_type: DamageType::BODY_PANEL,
            ..Default::default()
        });
        let telem = sample_damage(&dmg);
        assert!(telem.car_damage[0] > 0.0);
        assert!(telem.overall > 0.0);
        assert_eq!(telem.collision_count, 1);
    }

    #[test]
    fn repair_data_reports_replacement_and_cost() {
        let mut dmg = DamageSystem::new();
        dmg.set_config(DamageConfig {
            impact_threshold: 1.0,
            ..Default::default()
        });
        dmg.process_collision(CollisionEvent {
            contact_point: Vec3::new(0.0, 0.4, 2.5),
            impact_energy: 50000.0,
            damage_type: DamageType::BODY_PANEL,
            ..Default::default()
        });
        let repair = dmg.calculate_repair_data();
        assert!(repair.total_time() > 0.0);
        assert!(repair.total_repair_cost > 0.0);
    }
}
