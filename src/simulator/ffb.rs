//! Force feedback: coppia sul volante (soft + uscita hardware opzionale).
//!
//! Port di `src/simulator/FfbOutput.h` e `src/engine/devices/FFBBridge.{h,cpp}`
//! di ksengine. Il collegamento ai volanti Logitech/Fanatec/Moza resta
//! all'applicazione tramite il trait [`FfbActuator`]; senza hardware il modulo
//! produce solo la coppia "soft".

use super::tire::{MagicFormulaTire, TireState};

/// Ingressi del bridge FFB (port di `FFBInputs`).
#[derive(Clone, Copy, Debug)]
pub struct FfbInputs {
    /// Angolo di deriva anteriore sinistro (rad).
    pub slip_angle_fl: f32,
    /// Angolo di deriva anteriore destro (rad).
    pub slip_angle_fr: f32,
    /// Carico anteriore sinistro (N).
    pub load_fl: f32,
    /// Carico anteriore destro (N).
    pub load_fr: f32,
    /// Campanatura anteriore sinistra (rad).
    pub camber_fl: f32,
    /// Campanatura anteriore destra (rad).
    pub camber_fr: f32,
    /// Velocità (m/s).
    pub speed_ms: f32,
    /// Angolo di sterzo (rad).
    pub steer_angle: f32,
}

impl Default for FfbInputs {
    fn default() -> Self {
        Self {
            slip_angle_fl: 0.0,
            slip_angle_fr: 0.0,
            load_fl: 3500.0,
            load_fr: 3500.0,
            camber_fl: -0.03,
            camber_fr: -0.03,
            speed_ms: 0.0,
            steer_angle: 0.0,
        }
    }
}

/// Campione FFB del veicolo (port di `VehicleFFBSample` usato da `FfbOutput`).
#[derive(Clone, Copy, Debug)]
pub struct FfbSample {
    /// Angolo di deriva anteriore sinistro (rad).
    pub slip_angle_fl: f32,
    /// Angolo di deriva anteriore destro (rad).
    pub slip_angle_fr: f32,
    /// Carico anteriore sinistro (N).
    pub load_fl: f32,
    /// Carico anteriore destro (N).
    pub load_fr: f32,
    /// Campanatura anteriore sinistra (rad).
    pub camber_fl: f32,
    /// Campanatura anteriore destra (rad).
    pub camber_fr: f32,
    /// Velocità (m/s).
    pub speed_ms: f32,
    /// Angolo di sterzo (rad).
    pub steer_angle: f32,
    /// Momento di autoallineamento (N·m).
    pub aligning_moment_nm: f32,
}

impl Default for FfbSample {
    fn default() -> Self {
        Self {
            slip_angle_fl: 0.0,
            slip_angle_fr: 0.0,
            load_fl: 3500.0,
            load_fr: 3500.0,
            camber_fl: -0.03,
            camber_fr: -0.03,
            speed_ms: 0.0,
            steer_angle: 0.0,
            aligning_moment_nm: 0.0,
        }
    }
}

/// Attuatore hardware del volante, collegato dall'applicazione.
pub trait FfbActuator {
    /// Applica la coppia (N·m).
    fn apply(&mut self, torque_nm: f32);
}

/// Ponte momento di allineamento → coppia volante (port di `FFBBridge`).
pub struct FfbBridge;

impl FfbBridge {
    /// Calcola la coppia di sterzo (port di `computeSteeringTorque`).
    pub fn compute_steering_torque(
        inputs: &FfbInputs,
        tire_fl: Option<&mut MagicFormulaTire>,
        tire_fr: Option<&mut MagicFormulaTire>,
    ) -> f32 {
        let mz_l = mz_for(
            tire_fl,
            inputs.slip_angle_fl,
            inputs.load_fl,
            inputs.camber_fl,
        );
        let mz_r = mz_for(
            tire_fr,
            inputs.slip_angle_fr,
            inputs.load_fr,
            inputs.camber_fr,
        );
        let mz_total = (mz_l + mz_r) * 0.5;

        let load_scale = ((inputs.load_fl + inputs.load_fr) / 7000.0).clamp(0.3, 1.6);
        let speed_scale = (inputs.speed_ms / 30.0).clamp(0.0, 1.0);
        let scrub_radius = 0.03;
        let mech_trail = 0.02;

        let mut torque = (mz_total * 0.9
            + (inputs.load_fl + inputs.load_fr) * 0.5 * scrub_radius * 0.02 * inputs.steer_angle)
            * load_scale
            * (0.25 + 0.75 * speed_scale);
        torque += mech_trail
            * (inputs.load_fl + inputs.load_fr)
            * 0.001
            * inputs.steer_angle
            * speed_scale;
        torque.clamp(-25.0, 25.0)
    }

    /// Normalizza la coppia in `-1..=1` (port di `normalize`).
    pub fn normalize(torque_nm: f32) -> f32 {
        (torque_nm / 12.0).clamp(-1.0, 1.0)
    }
}

fn mz_for(tire: Option<&mut MagicFormulaTire>, slip: f32, load: f32, camber: f32) -> f32 {
    let Some(tire) = tire else {
        return 0.0;
    };
    let state = TireState {
        slip_angle: slip,
        slip_ratio: 0.0,
        normal_force: load.max(500.0),
        camber,
        mu: 1.0,
        pressure_psi: 26.0,
        temp_c: 90.0,
    };
    tire.calculate_forces(&state).mz
}

/// Uscita force feedback (port di `FfbOutput`).
pub struct FfbOutput {
    actuator: Option<Box<dyn FfbActuator>>,
    last_torque_nm: f32,
}

impl Default for FfbOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl FfbOutput {
    /// Un'uscita FFB senza hardware (solo soft).
    pub fn new() -> Self {
        Self {
            actuator: None,
            last_torque_nm: 0.0,
        }
    }

    /// "Inizializza" l'uscita: senza hardware resta FFB soft (port di
    /// `initialize`).
    pub fn initialize(&mut self) -> bool {
        true
    }

    /// Collega un attuatore hardware.
    pub fn set_actuator(&mut self, actuator: Box<dyn FfbActuator>) {
        self.actuator = Some(actuator);
    }

    /// Hardware disponibile.
    pub fn has_hardware(&self) -> bool {
        self.actuator.is_some()
    }

    /// Ultima coppia calcolata (N·m).
    pub fn last_torque_nm(&self) -> f32 {
        self.last_torque_nm
    }

    /// Rilascia l'attuatore (port di `shutdown`).
    pub fn shutdown(&mut self) {
        self.actuator = None;
    }

    /// Calcola e applica la coppia (port di `update`).
    pub fn update(
        &mut self,
        sample: &FfbSample,
        tire_fl: Option<&mut MagicFormulaTire>,
        tire_fr: Option<&mut MagicFormulaTire>,
        gain: f32,
    ) -> f32 {
        let inputs = FfbInputs {
            slip_angle_fl: sample.slip_angle_fl,
            slip_angle_fr: sample.slip_angle_fr,
            load_fl: sample.load_fl,
            load_fr: sample.load_fr,
            camber_fl: sample.camber_fl,
            camber_fr: sample.camber_fr,
            speed_ms: sample.speed_ms,
            steer_angle: sample.steer_angle,
        };

        let bridge_nm = FfbBridge::compute_steering_torque(&inputs, tire_fl, tire_fr);
        let mut torque_nm = 0.35 * (sample.aligning_moment_nm * 0.05) + 0.65 * bridge_nm;
        torque_nm *= gain.clamp(0.0, 2.0);
        if !torque_nm.is_finite() {
            torque_nm = 0.0;
        }
        torque_nm = torque_nm.clamp(-25.0, 25.0);
        self.last_torque_nm = torque_nm;

        if let Some(actuator) = self.actuator.as_mut() {
            actuator.apply(torque_nm);
        }
        torque_nm
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    struct RecordingActuator {
        last: Rc<Cell<f32>>,
    }

    impl FfbActuator for RecordingActuator {
        fn apply(&mut self, torque_nm: f32) {
            self.last.set(torque_nm);
        }
    }

    #[test]
    fn bridge_without_tires_is_zero() {
        let inputs = FfbInputs::default();
        let torque = FfbBridge::compute_steering_torque(&inputs, None, None);
        assert_eq!(torque, 0.0);
    }

    #[test]
    fn bridge_is_finite_and_clamped_with_tires() {
        let mut fl = MagicFormulaTire::slick();
        let mut fr = MagicFormulaTire::slick();
        let inputs = FfbInputs {
            slip_angle_fl: 0.15,
            slip_angle_fr: -0.15,
            load_fl: 4000.0,
            load_fr: 4000.0,
            speed_ms: 40.0,
            steer_angle: 0.1,
            ..FfbInputs::default()
        };
        let torque = FfbBridge::compute_steering_torque(&inputs, Some(&mut fl), Some(&mut fr));
        assert!(torque.is_finite());
        assert!(torque.abs() <= 25.0);
    }

    #[test]
    fn normalize_scales_and_clamps() {
        assert_eq!(FfbBridge::normalize(0.0), 0.0);
        assert!((FfbBridge::normalize(12.0) - 1.0).abs() < 1e-6);
        assert_eq!(FfbBridge::normalize(1000.0), 1.0);
        assert_eq!(FfbBridge::normalize(-1000.0), -1.0);
    }

    #[test]
    fn ffb_output_clamps_and_stores_torque() {
        let mut out = FfbOutput::new();
        assert!(out.initialize());
        assert!(!out.has_hardware());

        let sample = FfbSample {
            aligning_moment_nm: 1000.0,
            ..FfbSample::default()
        };
        let torque = out.update(&sample, None, None, 2.0);
        assert!(torque.is_finite());
        assert!(torque.abs() <= 25.0);
        assert_eq!(out.last_torque_nm(), torque);
    }

    #[test]
    fn ffb_output_drives_actuator() {
        let mut out = FfbOutput::new();
        let last = Rc::new(Cell::new(f32::NAN));
        out.set_actuator(Box::new(RecordingActuator {
            last: Rc::clone(&last),
        }));
        assert!(out.has_hardware());

        let sample = FfbSample {
            aligning_moment_nm: 500.0,
            ..FfbSample::default()
        };
        let torque = out.update(&sample, None, None, 1.0);
        assert_eq!(last.get(), torque);

        out.shutdown();
        assert!(!out.has_hardware());
    }

    #[test]
    fn non_finite_gain_is_guarded() {
        let mut out = FfbOutput::new();
        let sample = FfbSample {
            aligning_moment_nm: f32::NAN,
            ..FfbSample::default()
        };
        let torque = out.update(&sample, None, None, f32::INFINITY);
        assert_eq!(torque, 0.0);
    }
}
