//! Input: mappatura joystick/volante, tastiera e iniezione assi.
//!
//! Port di `src/simulator/InputManager.{h,cpp}` di ksengine. Il polling dei
//! dispositivi hardware (DirectInput / XInput) è dietro il trait
//! [`ControllerBackend`]: l'applicazione Windows lo collega, mentre i test e le
//! build non-Windows usano l'iniezione o la tastiera.

use std::collections::HashSet;

/// Numero di assi gestiti (port di `AXIS_COUNT`).
pub const AXIS_COUNT: usize = 8;
/// Asse stick sinistro X (port di `AXIS_LX`).
pub const AXIS_LX: usize = 0;
/// Asse stick sinistro Y (port di `AXIS_LY`).
pub const AXIS_LY: usize = 1;
/// Asse stick destro X (port di `AXIS_RX`).
pub const AXIS_RX: usize = 2;
/// Asse stick destro Y (port di `AXIS_RY`).
pub const AXIS_RY: usize = 3;
/// Asse grilletto sinistro (port di `AXIS_LT`).
pub const AXIS_LT: usize = 4;
/// Asse grilletto destro (port di `AXIS_RT`).
pub const AXIS_RT: usize = 5;

/// Mappatura di un joystick/volante (port di `JoystickMapping`).
#[derive(Clone, Copy, Debug)]
pub struct JoystickMapping {
    /// Asse dello sterzo.
    pub steer_axis: i32,
    /// Asse dell'acceleratore.
    pub throttle_axis: i32,
    /// Asse del freno.
    pub brake_axis: i32,
    /// Asse della frizione (-1 = assente).
    pub clutch_axis: i32,

    /// Inverte lo sterzo.
    pub invert_steer: bool,
    /// Inverte l'acceleratore.
    pub invert_throttle: bool,
    /// Inverte il freno.
    pub invert_brake: bool,
    /// Pedali combinati su un solo asse.
    pub throttle_is_combined: bool,

    /// Pulsante cambio su.
    pub shift_up_button: i32,
    /// Pulsante cambio giù.
    pub shift_down_button: i32,
    /// Pulsante frizione.
    pub clutch_button: i32,

    /// Zona morta.
    pub dead_zone: f64,
    /// Gamma dello sterzo.
    pub steer_gamma: f64,
    /// Gamma dell'acceleratore.
    pub throttle_gamma: f64,
    /// Gamma del freno.
    pub brake_gamma: f64,
}

impl Default for JoystickMapping {
    fn default() -> Self {
        Self::gamepad_default()
    }
}

impl JoystickMapping {
    /// Mappatura predefinita del gamepad (port di `gamepadDefault`).
    pub fn gamepad_default() -> Self {
        Self {
            steer_axis: 0,
            throttle_axis: 5,
            brake_axis: 4,
            clutch_axis: -1,
            invert_steer: false,
            invert_throttle: false,
            invert_brake: false,
            throttle_is_combined: false,
            shift_up_button: 5,
            shift_down_button: 4,
            clutch_button: 0,
            dead_zone: 0.12,
            steer_gamma: 1.6,
            throttle_gamma: 1.0,
            brake_gamma: 1.0,
        }
    }

    /// Mappatura predefinita del volante (port di `wheelDefault`).
    pub fn wheel_default() -> Self {
        Self {
            steer_axis: 0,
            throttle_axis: 1,
            brake_axis: 2,
            clutch_axis: 3,
            steer_gamma: 1.2,
            dead_zone: 0.02,
            shift_up_button: 4,
            shift_down_button: 5,
            ..Self::gamepad_default()
        }
    }

    /// Pedali combinati (port di `combinedPedal`).
    pub fn combined_pedal() -> Self {
        Self {
            throttle_is_combined: true,
            throttle_axis: 1,
            brake_axis: -1,
            ..Self::gamepad_default()
        }
    }
}

/// Mappatura della tastiera (port di `KeyboardMapping`), con le frecce come
/// alternative fisse.
#[derive(Clone, Copy, Debug)]
pub struct KeyboardMapping {
    /// Acceleratore.
    pub throttle: i32,
    /// Freno.
    pub brake: i32,
    /// Sterzo a sinistra.
    pub steer_left: i32,
    /// Sterzo a destra.
    pub steer_right: i32,
    /// Cambio su.
    pub shift_up: i32,
    /// Cambio giù.
    pub shift_down: i32,
    /// Freno a mano.
    pub handbrake: i32,
}

impl Default for KeyboardMapping {
    fn default() -> Self {
        Self {
            throttle: b'W' as i32,
            brake: b'S' as i32,
            steer_left: b'A' as i32,
            steer_right: b'D' as i32,
            shift_up: b'E' as i32,
            shift_down: b'Q' as i32,
            handbrake: 0x20,
        }
    }
}

impl KeyboardMapping {
    /// Acceleratore alternativo (freccia su).
    pub const ALT_THROTTLE: i32 = 0x26;
    /// Freno alternativo (freccia giù).
    pub const ALT_BRAKE: i32 = 0x28;
    /// Sterzo sinistra alternativo (freccia sinistra).
    pub const ALT_STEER_LEFT: i32 = 0x25;
    /// Sterzo destra alternativo (freccia destra).
    pub const ALT_STEER_RIGHT: i32 = 0x27;
}

const KEY_NAME_TABLE: &[(i32, &str)] = &[
    (0x20, "SPACE"),
    (0x0D, "ENTER"),
    (0x1B, "ESC"),
    (0x09, "TAB"),
    (0x25, "LEFT"),
    (0x26, "UP"),
    (0x27, "RIGHT"),
    (0x28, "DOWN"),
    (0x10, "SHIFT"),
    (0x11, "CTRL"),
    (0x12, "ALT"),
    (0x24, "HOME"),
    (0x23, "END"),
    (0x21, "PGUP"),
    (0x22, "PGDN"),
    (0x2D, "INSERT"),
    (0x2E, "DELETE"),
];

/// Nome stabile di un codice virtuale (port di `keyName`).
pub fn key_name(vk: i32) -> String {
    if (b'A' as i32..=b'Z' as i32).contains(&vk) {
        return char::from(vk as u8).to_string();
    }
    if (b'0' as i32..=b'9' as i32).contains(&vk) {
        return char::from(vk as u8).to_string();
    }
    if (0x70..=0x7B).contains(&vk) {
        return format!("F{}", vk - 0x6F);
    }
    for &(code, name) in KEY_NAME_TABLE {
        if code == vk {
            return name.to_string();
        }
    }
    format!("VK{vk}")
}

/// Inverso di [`key_name`], senza distinzione maiuscole/minuscole; -1 se
/// sconosciuto (port di `keyFromName`).
pub fn key_from_name(name: &str) -> i32 {
    let up = name.trim().to_uppercase();
    if up.len() == 1 {
        let c = up.as_bytes()[0];
        if c.is_ascii_uppercase() || c.is_ascii_digit() {
            return c as i32;
        }
        return -1;
    }
    if up.len() >= 2 && up.starts_with('F') {
        if let Ok(f) = up[1..].parse::<i32>() {
            if (1..=12).contains(&f) {
                return 0x70 + (f - 1);
            }
        }
    }
    for &(code, n) in KEY_NAME_TABLE {
        if up == n {
            return code;
        }
    }
    if let Some(rest) = up.strip_prefix("VK") {
        if let Ok(vk) = rest.parse::<i32>() {
            if vk > 0 && vk < 256 {
                return vk;
            }
        }
    }
    -1
}

/// Salva la mappatura della tastiera in un file ini piatto (port di
/// `saveKeyboardMapping`). `false` se non scrivibile.
pub fn save_keyboard_mapping(path: &std::path::Path, m: &KeyboardMapping) -> bool {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() && std::fs::create_dir_all(parent).is_err() {
            return false;
        }
    }
    let text = format!(
        "# ksengine keyboard bindings\n\
         throttle={}\nbrake={}\nsteerLeft={}\nsteerRight={}\n\
         shiftUp={}\nshiftDown={}\nhandbrake={}\n",
        key_name(m.throttle),
        key_name(m.brake),
        key_name(m.steer_left),
        key_name(m.steer_right),
        key_name(m.shift_up),
        key_name(m.shift_down),
        key_name(m.handbrake),
    );
    std::fs::write(path, text).is_ok()
}

/// Carica la mappatura della tastiera sopra una esistente (port di
/// `loadKeyboardMapping`): file mancante → `false` senza modifiche; righe
/// malformate e nomi sconosciuti vengono saltati singolarmente.
pub fn load_keyboard_mapping(path: &std::path::Path, m: &mut KeyboardMapping) -> bool {
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
        let vk = key_from_name(&s[eq + 1..]);
        if vk < 0 {
            continue;
        }
        match &s[..eq] {
            "throttle" => m.throttle = vk,
            "brake" => m.brake = vk,
            "steerLeft" => m.steer_left = vk,
            "steerRight" => m.steer_right = vk,
            "shiftUp" => m.shift_up = vk,
            "shiftDown" => m.shift_down = vk,
            "handbrake" => m.handbrake = vk,
            _ => {}
        }
    }
    true
}

/// Stato pollato di un controller.
#[derive(Clone, Copy, Debug)]
pub struct ControllerState {
    /// Assi normalizzati.
    pub axes: [f64; AXIS_COUNT],
    /// Bitmask dei pulsanti.
    pub buttons: u32,
}

impl Default for ControllerState {
    fn default() -> Self {
        Self {
            axes: [0.0; AXIS_COUNT],
            buttons: 0,
        }
    }
}

/// Backend di polling di un dispositivo (collegato dall'applicazione).
pub trait ControllerBackend {
    /// Il dispositivo è connesso.
    fn is_connected(&self) -> bool;
    /// Legge lo stato corrente.
    fn poll(&mut self) -> ControllerState;
}

fn poll_if_connected(backend: &mut Option<Box<dyn ControllerBackend>>) -> Option<ControllerState> {
    let b = backend.as_mut()?;
    if b.is_connected() {
        Some(b.poll())
    } else {
        None
    }
}

/// Gestore dell'input (port di `InputManager`).
pub struct InputManager {
    throttle: f64,
    brake: f64,
    steer: f64,
    clutch: f64,
    raw_throttle: f64,
    raw_brake: f64,
    raw_steer: f64,

    shift_up: bool,
    shift_down: bool,
    prev_shift_up_key: bool,
    prev_shift_down_key: bool,
    prev_shift_up_btn: bool,
    prev_shift_down_btn: bool,

    map: JoystickMapping,
    kb: KeyboardMapping,

    axes: [f64; AXIS_COUNT],
    buttons: u32,
    has_injected: bool,

    keys: HashSet<i32>,

    wheel: Option<Box<dyn ControllerBackend>>,
    gamepad: Option<Box<dyn ControllerBackend>>,
    prefer_di: bool,
}

impl Default for InputManager {
    fn default() -> Self {
        Self::new()
    }
}

impl InputManager {
    /// Tasto freccia su (port di `KEY_UP`).
    pub const KEY_UP: i32 = 0x26;
    /// Tasto freccia giù (port di `KEY_DOWN`).
    pub const KEY_DOWN: i32 = 0x28;
    /// Tasto freccia sinistra (port di `KEY_LEFT`).
    pub const KEY_LEFT: i32 = 0x25;
    /// Tasto freccia destra (port di `KEY_RIGHT`).
    pub const KEY_RIGHT: i32 = 0x27;

    /// Un gestore con mappatura gamepad predefinita.
    pub fn new() -> Self {
        Self {
            throttle: 0.0,
            brake: 0.0,
            steer: 0.0,
            clutch: 0.0,
            raw_throttle: 0.0,
            raw_brake: 0.0,
            raw_steer: 0.0,
            shift_up: false,
            shift_down: false,
            prev_shift_up_key: false,
            prev_shift_down_key: false,
            prev_shift_up_btn: false,
            prev_shift_down_btn: false,
            map: JoystickMapping::gamepad_default(),
            kb: KeyboardMapping::default(),
            axes: [0.0; AXIS_COUNT],
            buttons: 0,
            has_injected: false,
            keys: HashSet::new(),
            wheel: None,
            gamepad: None,
            prefer_di: true,
        }
    }

    /// Inizializza (senza hardware: i backend vengono collegati dall'app).
    pub fn initialize(&mut self) -> bool {
        true
    }

    /// Collega il backend del volante.
    pub fn set_wheel_backend(&mut self, backend: Box<dyn ControllerBackend>) {
        self.wheel = Some(backend);
    }

    /// Collega il backend del gamepad.
    pub fn set_gamepad_backend(&mut self, backend: Box<dyn ControllerBackend>) {
        self.gamepad = Some(backend);
    }

    /// Acceleratore.
    pub fn throttle(&self) -> f64 {
        self.throttle
    }
    /// Freno.
    pub fn brake(&self) -> f64 {
        self.brake
    }
    /// Sterzo.
    pub fn steer(&self) -> f64 {
        self.steer
    }
    /// Frizione.
    pub fn clutch(&self) -> f64 {
        self.clutch
    }
    /// Acceleratore grezzo.
    pub fn raw_throttle(&self) -> f64 {
        self.raw_throttle
    }
    /// Freno grezzo.
    pub fn raw_brake(&self) -> f64 {
        self.raw_brake
    }
    /// Sterzo grezzo.
    pub fn raw_steer(&self) -> f64 {
        self.raw_steer
    }
    /// Cambio su (edge).
    pub fn shift_up(&self) -> bool {
        self.shift_up
    }
    /// Cambio giù (edge).
    pub fn shift_down(&self) -> bool {
        self.shift_down
    }

    /// Volante presente.
    pub fn has_wheel(&self) -> bool {
        self.wheel.is_some()
    }
    /// Volante connesso.
    pub fn is_wheel_connected(&self) -> bool {
        self.wheel.as_ref().is_some_and(|b| b.is_connected())
    }
    /// Gamepad presente.
    pub fn has_gamepad(&self) -> bool {
        self.gamepad.is_some()
    }
    /// Gamepad connesso.
    pub fn is_gamepad_connected(&self) -> bool {
        self.gamepad.as_ref().is_some_and(|b| b.is_connected())
    }

    /// Preferisci il volante DirectInput.
    pub fn set_prefer_direct_input(&mut self, value: bool) {
        self.prefer_di = value;
    }
    /// Preferenza volante DirectInput.
    pub fn prefer_direct_input(&self) -> bool {
        self.prefer_di
    }

    /// Segnala un tasto premuto.
    pub fn set_key_down(&mut self, key: i32) {
        self.keys.insert(key);
    }
    /// Segnala un tasto rilasciato.
    pub fn set_key_up(&mut self, key: i32) {
        self.keys.remove(&key);
    }
    /// Il tasto è premuto.
    pub fn is_key_down(&self, key: i32) -> bool {
        self.keys.contains(&key)
    }

    /// Imposta la mappatura joystick.
    pub fn set_mapping(&mut self, map: JoystickMapping) {
        self.map = map;
    }
    /// La mappatura joystick.
    pub fn mapping(&self) -> &JoystickMapping {
        &self.map
    }
    /// Imposta la gamma dello sterzo.
    pub fn set_steer_gamma(&mut self, gamma: f64) {
        self.map.steer_gamma = gamma;
    }
    /// Imposta la zona morta.
    pub fn set_dead_zone(&mut self, dead_zone: f64) {
        self.map.dead_zone = dead_zone;
    }
    /// Inverte lo sterzo.
    pub fn set_invert_steer(&mut self, invert: bool) {
        self.map.invert_steer = invert;
    }

    /// Imposta la mappatura della tastiera.
    pub fn set_keyboard_mapping(&mut self, kb: KeyboardMapping) {
        self.kb = kb;
    }
    /// La mappatura della tastiera.
    pub fn keyboard_mapping(&self) -> &KeyboardMapping {
        &self.kb
    }

    /// Inietta assi e pulsanti.
    pub fn inject_axes(&mut self, axes: [f64; AXIS_COUNT], buttons: u32) {
        self.axes = axes;
        self.buttons = buttons;
        self.has_injected = true;
    }

    /// Inietta un singolo asse.
    pub fn inject_axis(&mut self, index: usize, value: f64) {
        if index >= AXIS_COUNT {
            return;
        }
        self.axes[index] = value;
        self.has_injected = true;
    }

    /// Inietta un pulsante.
    pub fn inject_button(&mut self, index: usize, pressed: bool) {
        if index >= 32 {
            return;
        }
        if pressed {
            self.buttons |= 1 << index;
        } else {
            self.buttons &= !(1 << index);
        }
        self.has_injected = true;
    }

    /// Azzera lo stato.
    pub fn reset(&mut self) {
        self.throttle = 0.0;
        self.brake = 0.0;
        self.steer = 0.0;
        self.clutch = 0.0;
        self.raw_throttle = 0.0;
        self.raw_brake = 0.0;
        self.raw_steer = 0.0;
        self.shift_up = false;
        self.shift_down = false;
        self.keys.clear();
        self.buttons = 0;
        self.has_injected = false;
        self.axes = [0.0; AXIS_COUNT];
    }

    /// Avanza lo stato di un frame (port di `update`).
    pub fn update(&mut self) {
        self.shift_up = false;
        self.shift_down = false;

        if self.has_injected {
            self.process_injected();
            self.has_injected = false;
            return;
        }

        if self.prefer_di {
            if let Some(state) = poll_if_connected(&mut self.wheel) {
                self.apply_mapping(&state.axes, state.buttons);
                return;
            }
        }
        if let Some(state) = poll_if_connected(&mut self.gamepad) {
            self.apply_mapping(&state.axes, state.buttons);
            if state.buttons & (1 << 8) != 0 {
                self.shift_up = true;
            }
            if state.buttons & (1 << 9) != 0 {
                self.shift_down = true;
            }
            return;
        }
        if let Some(state) = poll_if_connected(&mut self.wheel) {
            self.apply_mapping(&state.axes, state.buttons);
            return;
        }

        self.process_keyboard();
    }

    /// Curva con zona morta e gamma (port di `curve`).
    pub fn curve(v: f64, gamma: f64, dead_zone: f64, bipolar: bool) -> f64 {
        if bipolar {
            if v.abs() < dead_zone {
                return 0.0;
            }
            let sign = if v > 0.0 { 1.0 } else { -1.0 };
            let n = (v.abs() - dead_zone) / (1.0 - dead_zone);
            return sign * n.clamp(0.0, 1.0).powf(gamma);
        }
        if v < dead_zone {
            return 0.0;
        }
        let n = (v - dead_zone) / (1.0 - dead_zone);
        n.clamp(0.0, 1.0).powf(gamma)
    }

    fn process_injected(&mut self) {
        let axes = self.axes;
        let buttons = self.buttons;
        self.apply_mapping(&axes, buttons);
    }

    fn apply_mapping(&mut self, axes: &[f64; AXIS_COUNT], buttons: u32) {
        let axis = |idx: i32| -> f64 {
            if idx < 0 || idx as usize >= AXIS_COUNT {
                return 0.0;
            }
            axes[idx as usize]
        };

        let mut st = axis(self.map.steer_axis);
        st = Self::curve(st, self.map.steer_gamma, self.map.dead_zone, true);
        if self.map.invert_steer {
            st = -st;
        }
        self.steer = st.clamp(-1.0, 1.0);

        if self.map.throttle_is_combined {
            let a = axis(self.map.throttle_axis);
            if a >= 0.0 {
                self.throttle = Self::curve(a, self.map.throttle_gamma, self.map.dead_zone, false);
                self.brake = 0.0;
            } else {
                self.brake = Self::curve(-a, self.map.brake_gamma, self.map.dead_zone, false);
                self.throttle = 0.0;
            }
        } else {
            let mut th = axis(self.map.throttle_axis);
            let mut br = axis(self.map.brake_axis);
            if th < 0.0 {
                th = 0.0;
            }
            if br < 0.0 {
                br = 0.0;
            }
            if self.map.invert_throttle {
                th = 1.0 - th;
            }
            if self.map.invert_brake {
                br = 1.0 - br;
            }
            self.throttle = Self::curve(th, self.map.throttle_gamma, 0.02, false);
            self.brake = Self::curve(br, self.map.brake_gamma, 0.02, false);
        }

        if self.map.clutch_axis >= 0 {
            let mut c = axis(self.map.clutch_axis);
            if c < 0.0 {
                c = 0.0;
            }
            self.clutch = Self::curve(c, 1.0, 0.02, false);
        } else if self.map.clutch_button >= 0 {
            self.clutch = if buttons & (1 << self.map.clutch_button) != 0 {
                1.0
            } else {
                0.0
            };
        } else {
            self.clutch = 0.0;
        }

        let up = self.map.shift_up_button >= 0 && buttons & (1u32 << self.map.shift_up_button) != 0;
        let dn =
            self.map.shift_down_button >= 0 && buttons & (1u32 << self.map.shift_down_button) != 0;
        if up && !self.prev_shift_up_btn {
            self.shift_up = true;
        }
        if dn && !self.prev_shift_down_btn {
            self.shift_down = true;
        }
        self.prev_shift_up_btn = up;
        self.prev_shift_down_btn = dn;

        self.raw_throttle = self.throttle;
        self.raw_brake = self.brake;
        self.raw_steer = self.steer;
    }

    fn process_keyboard(&mut self) {
        let norm = |k: i32| {
            if (b'a' as i32..=b'z' as i32).contains(&k) {
                k - (b'a' as i32 - b'A' as i32)
            } else {
                k
            }
        };
        let self_keys = &self.keys;
        let pressed = |vk: i32| -> bool {
            if vk <= 0 {
                return false;
            }
            let up = norm(vk);
            if self_keys.contains(&up) {
                return true;
            }
            if (b'A' as i32..=b'Z' as i32).contains(&up) {
                return self_keys.contains(&(up + (b'a' as i32 - b'A' as i32)));
            }
            false
        };

        self.throttle = if pressed(self.kb.throttle) || pressed(KeyboardMapping::ALT_THROTTLE) {
            1.0
        } else {
            0.0
        };
        self.brake = if pressed(self.kb.brake) || pressed(KeyboardMapping::ALT_BRAKE) {
            1.0
        } else {
            0.0
        };

        self.steer = 0.0;
        if pressed(self.kb.steer_left) || pressed(KeyboardMapping::ALT_STEER_LEFT) {
            self.steer -= 1.0;
        }
        if pressed(self.kb.steer_right) || pressed(KeyboardMapping::ALT_STEER_RIGHT) {
            self.steer += 1.0;
        }

        if self.map.invert_steer {
            self.steer = -self.steer;
        }

        let cur_up = pressed(self.kb.shift_up);
        let cur_dn = pressed(self.kb.shift_down);
        if cur_up && !self.prev_shift_up_key {
            self.shift_up = true;
        }
        if cur_dn && !self.prev_shift_down_key {
            self.shift_down = true;
        }
        self.prev_shift_up_key = cur_up;
        self.prev_shift_down_key = cur_dn;

        self.steer = Self::curve(self.steer, self.map.steer_gamma, self.map.dead_zone, true);
        self.steer = self.steer.clamp(-1.0, 1.0);
        self.throttle = self.throttle.clamp(0.0, 1.0);
        self.brake = self.brake.clamp(0.0, 1.0);

        self.raw_throttle = self.throttle;
        self.raw_brake = self.brake;
        self.raw_steer = self.steer;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    struct MockBackend {
        connected: bool,
        state: ControllerState,
        polls: Rc<Cell<u32>>,
    }

    impl ControllerBackend for MockBackend {
        fn is_connected(&self) -> bool {
            self.connected
        }
        fn poll(&mut self) -> ControllerState {
            self.polls.set(self.polls.get() + 1);
            self.state
        }
    }

    fn backend(
        connected: bool,
        state: ControllerState,
    ) -> (Box<dyn ControllerBackend>, Rc<Cell<u32>>) {
        let polls = Rc::new(Cell::new(0));
        (
            Box::new(MockBackend {
                connected,
                state,
                polls: Rc::clone(&polls),
            }),
            polls,
        )
    }

    #[test]
    fn key_name_and_from_name_round_trip() {
        for vk in ['A' as i32, '5' as i32, 0x20, 0x25, 0x70, 0x7B, 0x43] {
            let name = key_name(vk);
            assert_eq!(key_from_name(&name), vk, "name = {name}");
        }
        assert_eq!(key_name(0x70), "F1");
        assert_eq!(key_from_name("f12"), 0x7B);
        assert_eq!(key_from_name("nonsense"), -1);
    }

    #[test]
    fn curve_applies_dead_zone_and_gamma() {
        assert_eq!(InputManager::curve(0.05, 1.0, 0.15, true), 0.0);
        assert!((InputManager::curve(1.0, 1.0, 0.15, true) - 1.0).abs() < 1e-9);
        let mid = InputManager::curve(0.575, 1.0, 0.15, true);
        assert!((mid - 0.5).abs() < 1e-9, "mid = {mid}");
        // Negative input keeps its sign.
        assert!(InputManager::curve(-0.575, 1.0, 0.15, true) < 0.0);
    }

    #[test]
    fn wheel_mapping_reads_axes() {
        let mut im = InputManager::new();
        im.set_mapping(JoystickMapping::wheel_default());
        let mut axes = [0.0; AXIS_COUNT];
        axes[0] = 0.8;
        axes[1] = 1.0;
        axes[2] = 0.0;
        axes[3] = 0.5;
        im.inject_axes(axes, 0);
        im.update();
        assert!(im.steer() > 0.0);
        assert!(im.throttle() > 0.9);
        assert_eq!(im.brake(), 0.0);
        assert!(im.clutch() > 0.0);
    }

    #[test]
    fn invert_steer_flips() {
        let mut im = InputManager::new();
        im.set_mapping(JoystickMapping {
            invert_steer: true,
            ..JoystickMapping::wheel_default()
        });
        let mut axes = [0.0; AXIS_COUNT];
        axes[0] = 0.9;
        im.inject_axes(axes, 0);
        im.update();
        assert!(im.steer() < 0.0);
    }

    #[test]
    fn combined_pedal_splits_sign() {
        let mut im = InputManager::new();
        im.set_mapping(JoystickMapping::combined_pedal());
        let mut axes = [0.0; AXIS_COUNT];
        axes[1] = 0.7;
        im.inject_axes(axes, 0);
        im.update();
        assert!(im.throttle() > 0.0);
        assert_eq!(im.brake(), 0.0);

        let mut axes2 = [0.0; AXIS_COUNT];
        axes2[1] = -0.7;
        im.inject_axes(axes2, 0);
        im.update();
        assert_eq!(im.throttle(), 0.0);
        assert!(im.brake() > 0.0);
    }

    #[test]
    fn shift_buttons_are_edge_triggered() {
        let mut im = InputManager::new();
        im.set_mapping(JoystickMapping::wheel_default());
        im.inject_axes([0.0; AXIS_COUNT], 1 << 4);
        im.update();
        assert!(im.shift_up());
        // Held down on the next frame must not re-trigger.
        im.inject_axes([0.0; AXIS_COUNT], 1 << 4);
        im.update();
        assert!(!im.shift_up());
    }

    #[test]
    fn keyboard_drives_throttle_and_steer() {
        let mut im = InputManager::new();
        im.set_key_down(b'W' as i32);
        im.set_key_down(b'A' as i32);
        im.update();
        assert_eq!(im.throttle(), 1.0);
        assert!(im.steer() < 0.0);
        im.set_key_up(b'W' as i32);
        im.update();
        assert_eq!(im.throttle(), 0.0);
    }

    #[test]
    fn keyboard_shift_edges() {
        let mut im = InputManager::new();
        im.set_key_down(b'E' as i32);
        im.update();
        assert!(im.shift_up());
        im.update();
        assert!(!im.shift_up());
    }

    #[test]
    fn prefer_di_uses_wheel_backend() {
        let mut im = InputManager::new();
        im.set_mapping(JoystickMapping::wheel_default());
        let mut axes = [0.0; AXIS_COUNT];
        axes[1] = 1.0;
        let (wheel, wheel_polls) = backend(true, ControllerState { axes, buttons: 0 });
        let (pad, pad_polls) = backend(true, ControllerState::default());
        im.set_wheel_backend(wheel);
        im.set_gamepad_backend(pad);
        im.update();
        assert_eq!(wheel_polls.get(), 1);
        assert_eq!(pad_polls.get(), 0);
        assert!(im.throttle() > 0.9);
    }

    #[test]
    fn falls_back_to_gamepad_then_keyboard() {
        let mut im = InputManager::new();
        let (wheel, _) = backend(false, ControllerState::default());
        let mut axes = [0.0; AXIS_COUNT];
        axes[5] = 1.0; // right trigger
        let (pad, pad_polls) = backend(true, ControllerState { axes, buttons: 0 });
        im.set_wheel_backend(wheel);
        im.set_gamepad_backend(pad);
        im.update();
        assert_eq!(pad_polls.get(), 1);
        assert!(im.throttle() > 0.9);

        im.set_key_down(InputManager::KEY_UP);
        im.update();
        assert_eq!(im.throttle(), 1.0);
    }

    #[test]
    fn reset_clears_everything() {
        let mut im = InputManager::new();
        im.inject_axis(0, 0.9);
        im.update();
        assert!(im.steer() != 0.0);
        im.reset();
        assert_eq!(im.steer(), 0.0);
        assert_eq!(im.throttle(), 0.0);
        assert!(!im.is_key_down(b'W' as i32));
    }

    #[test]
    fn save_load_keyboard_mapping_round_trip() {
        let dir = std::env::temp_dir().join("racingedge_input_test");
        let path = dir.join("keyboard.ini");
        let mapping = KeyboardMapping {
            throttle: b'I' as i32,
            brake: b'K' as i32,
            steer_left: b'J' as i32,
            steer_right: b'L' as i32,
            shift_up: InputManager::KEY_UP,
            shift_down: InputManager::KEY_DOWN,
            handbrake: 0x10,
        };
        assert!(save_keyboard_mapping(&path, &mapping));

        let mut loaded = KeyboardMapping::default();
        assert!(load_keyboard_mapping(&path, &mut loaded));
        assert_eq!(loaded.throttle, b'I' as i32);
        assert_eq!(loaded.shift_up, InputManager::KEY_UP);
        assert_eq!(loaded.shift_down, InputManager::KEY_DOWN);
        assert_eq!(loaded.handbrake, 0x10);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn load_missing_mapping_returns_false() {
        let mut mapping = KeyboardMapping::default();
        let missing = std::env::temp_dir().join("racingedge_input_test/nope.ini");
        assert!(!load_keyboard_mapping(&missing, &mut mapping));
        assert_eq!(mapping.throttle, b'W' as i32);
    }

    #[test]
    fn load_skips_unknown_names_and_lines() {
        let dir = std::env::temp_dir().join("racingedge_input_test");
        let path = dir.join("partial.ini");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &path,
            "# comment\ngarbage line\nthrottle=BOGUS\nbrake=SPACE\n",
        )
        .unwrap();
        let mut mapping = KeyboardMapping::default();
        assert!(load_keyboard_mapping(&path, &mut mapping));
        assert_eq!(mapping.throttle, b'W' as i32, "BOGUS must be skipped");
        assert_eq!(mapping.brake, 0x20);
        let _ = std::fs::remove_file(&path);
    }
}
