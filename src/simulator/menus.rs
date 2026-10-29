//! Menus: pause, settings and key rebind.
//!
//! Port of ksengine `src/simulator/GameMenuOverlay.{h,cpp}`,
//! `src/simulator/KeyRebindOverlay.h` and `src/simulator/DeviceSettings.h`
//! (GPL-3.0). Original: <https://github.com/SimonTek27/ksengine>
//!
//! As with the rest of the ported simulator, the Qt overlay becomes a Bevy
//! screen (`crate::plugins::menu`) and everything that decides *what* the menu
//! shows and does stays here in a plain model, so it is testable without a
//! renderer:
//!
//! | C++ | RacingEdge |
//! | --- | --- |
//! | `GameMenuOverlay::Screen` | [`MenuScreen`] |
//! | `GameMenuOverlay::entries` | [`GameMenu::entries`] |
//! | `GameMenuOverlay::activate` | [`GameMenu::activate`] |
//! | `KeyRebindOverlay` capture | [`GameMenu::begin_rebind`], [`GameMenu::capture_key`] |
//! | `DeviceSettings` | [`MenuSettings`] |
//!
//! The settings and the key map persist in the same flat INI style the rest of
//! the content uses ([`crate::editor::ini`]), and the key map is the shared
//! [`KeyboardMapping`] from [`crate::simulator::input`] — one binding model for
//! the menu, the drive loop and the C API hosts.
//!
//! # Example
//!
//! ```
//! use racingedge::simulator::menus::{GameMenu, MenuAction, MenuScreen};
//!
//! let mut menu = GameMenu::new();
//! menu.open_pause();
//! assert_eq!(menu.screen(), MenuScreen::Pause);
//! assert!(menu.is_paused());
//!
//! // The first entry is "Resume".
//! assert_eq!(menu.activate(), Some(MenuAction::Resume));
//! assert!(!menu.is_open());
//! ```

use std::path::Path;

use super::camera::CameraMode;
use super::hud::RaceHudMode;
use super::input::{key_name, load_keyboard_mapping, save_keyboard_mapping, KeyboardMapping};
use crate::editor::ini::Ini;

/// Where the menu settings are persisted (`DeviceSettings` defaults).
pub const SETTINGS_PATH: &str = "user/settings.ini";
/// Where the rebindable driving keys are persisted (`KeyRebindOverlay`).
pub const KEYBOARD_PATH: &str = "user/keyboard.ini";

/// Which screen the menu is showing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MenuScreen {
    /// The menu is not drawn and the simulation runs.
    #[default]
    Closed,
    /// The pause screen (resume, settings, key bindings, restart, quit).
    Pause,
    /// Device/driver settings.
    Settings,
    /// Key rebinding.
    KeyRebind,
}

impl MenuScreen {
    /// Title drawn above the entries (port of `screenTitle`).
    pub fn title(self) -> &'static str {
        match self {
            Self::Closed => "",
            Self::Pause => "PAUSED",
            Self::Settings => "SETTINGS",
            Self::KeyRebind => "KEY BINDINGS",
        }
    }
}

/// A driving action whose key can be rebound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BindingAction {
    /// Accelerator.
    Throttle,
    /// Brake.
    Brake,
    /// Steer left.
    SteerLeft,
    /// Steer right.
    SteerRight,
    /// Shift up.
    ShiftUp,
    /// Shift down.
    ShiftDown,
    /// Handbrake.
    Handbrake,
}

impl BindingAction {
    /// Every rebindable action, in menu order.
    pub const ALL: [BindingAction; 7] = [
        Self::Throttle,
        Self::Brake,
        Self::SteerLeft,
        Self::SteerRight,
        Self::ShiftUp,
        Self::ShiftDown,
        Self::Handbrake,
    ];

    /// The row label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Throttle => "Throttle",
            Self::Brake => "Brake",
            Self::SteerLeft => "Steer left",
            Self::SteerRight => "Steer right",
            Self::ShiftUp => "Shift up",
            Self::ShiftDown => "Shift down",
            Self::Handbrake => "Handbrake",
        }
    }

    /// The virtual-key code currently bound to this action.
    pub fn key(self, mapping: &KeyboardMapping) -> i32 {
        match self {
            Self::Throttle => mapping.throttle,
            Self::Brake => mapping.brake,
            Self::SteerLeft => mapping.steer_left,
            Self::SteerRight => mapping.steer_right,
            Self::ShiftUp => mapping.shift_up,
            Self::ShiftDown => mapping.shift_down,
            Self::Handbrake => mapping.handbrake,
        }
    }

    /// Rebinds this action to `vk`.
    pub fn set(self, mapping: &mut KeyboardMapping, vk: i32) {
        match self {
            Self::Throttle => mapping.throttle = vk,
            Self::Brake => mapping.brake = vk,
            Self::SteerLeft => mapping.steer_left = vk,
            Self::SteerRight => mapping.steer_right = vk,
            Self::ShiftUp => mapping.shift_up = vk,
            Self::ShiftDown => mapping.shift_down = vk,
            Self::Handbrake => mapping.handbrake = vk,
        }
    }
}

/// A device/driver setting row (port of `DeviceSettings`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingField {
    /// Master output volume.
    MasterVolume,
    /// Race HUD mode.
    HudMode,
    /// Camera mode.
    Camera,
    /// Force-feedback gain.
    FfbGain,
    /// Steering gamma curve.
    SteerGamma,
    /// Steering dead zone.
    SteerDeadZone,
    /// Invert the steering axis.
    InvertSteer,
}

impl SettingField {
    /// Every setting row, in menu order.
    pub const ALL: [SettingField; 7] = [
        Self::MasterVolume,
        Self::HudMode,
        Self::Camera,
        Self::FfbGain,
        Self::SteerGamma,
        Self::SteerDeadZone,
        Self::InvertSteer,
    ];

    /// The row label.
    pub fn label(self) -> &'static str {
        match self {
            Self::MasterVolume => "Master volume",
            Self::HudMode => "HUD mode",
            Self::Camera => "Camera",
            Self::FfbGain => "FFB gain",
            Self::SteerGamma => "Steering gamma",
            Self::SteerDeadZone => "Steering dead zone",
            Self::InvertSteer => "Invert steering",
        }
    }
}

/// The mutable game/device settings the menu edits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MenuSettings {
    /// Master output volume (`0..=1`).
    pub master_volume: f32,
    /// Race HUD mode.
    pub hud_mode: RaceHudMode,
    /// Camera mode the drive loop starts in.
    pub camera_mode: CameraMode,
    /// Force-feedback gain (`0..=1`).
    pub ffb_gain: f32,
    /// Steering gamma (`0.5..=2.0`).
    pub steer_gamma: f64,
    /// Steering dead zone (`0..=0.4`).
    pub steer_dead_zone: f64,
    /// Invert the steering axis.
    pub invert_steer: bool,
}

impl Default for MenuSettings {
    fn default() -> Self {
        Self {
            master_volume: 0.8,
            hud_mode: RaceHudMode::Race,
            camera_mode: CameraMode::Chase,
            ffb_gain: 1.0,
            steer_gamma: 1.0,
            steer_dead_zone: 0.05,
            invert_steer: false,
        }
    }
}

impl MenuSettings {
    /// Clamps every value into its supported range.
    pub fn clamp(&mut self) {
        self.master_volume = self.master_volume.clamp(0.0, 1.0);
        self.ffb_gain = self.ffb_gain.clamp(0.0, 1.0);
        self.steer_gamma = self.steer_gamma.clamp(0.5, 2.0);
        self.steer_dead_zone = self.steer_dead_zone.clamp(0.0, 0.4);
    }

    /// Steps `field` by `dir` (`+1` forward, `-1` back), clamping afterwards.
    pub fn adjust(&mut self, field: SettingField, dir: i32) {
        let forward = dir >= 0;
        match field {
            SettingField::MasterVolume => {
                self.master_volume += if forward { 0.05 } else { -0.05 };
            }
            SettingField::FfbGain => {
                self.ffb_gain += if forward { 0.1 } else { -0.1 };
            }
            SettingField::SteerGamma => {
                self.steer_gamma += if forward { 0.1 } else { -0.1 };
            }
            SettingField::SteerDeadZone => {
                self.steer_dead_zone += if forward { 0.01 } else { -0.01 };
            }
            SettingField::HudMode => {
                self.hud_mode = cycle_hud_mode(self.hud_mode, forward);
            }
            SettingField::Camera => {
                self.camera_mode = cycle_camera_mode(self.camera_mode, forward);
            }
            SettingField::InvertSteer => {
                self.invert_steer = !self.invert_steer;
            }
        }
        self.clamp();
    }

    /// The value shown on the right of a settings row.
    pub fn value_label(&self, field: SettingField) -> String {
        match field {
            SettingField::MasterVolume => format!("{:.0}%", self.master_volume * 100.0),
            SettingField::FfbGain => format!("{:.0}%", self.ffb_gain * 100.0),
            SettingField::SteerGamma => format!("{:.2}", self.steer_gamma),
            SettingField::SteerDeadZone => format!("{:.2}", self.steer_dead_zone),
            SettingField::HudMode => hud_mode_name(self.hud_mode).to_string(),
            SettingField::Camera => camera_mode_name(self.camera_mode).to_string(),
            SettingField::InvertSteer => {
                if self.invert_steer {
                    "on".to_string()
                } else {
                    "off".to_string()
                }
            }
        }
    }

    /// Writes the settings as an INI file, creating parent directories.
    /// `false` when the file cannot be written.
    pub fn save(&self, path: impl AsRef<Path>) -> bool {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && std::fs::create_dir_all(parent).is_err() {
                return false;
            }
        }
        let text = format!(
            "# racingedge menu settings\n\
             [AUDIO]\n\
             masterVolume={:.2}\n\
             [GRAPHICS]\n\
             hudMode={}\n\
             camera={}\n\
             [INPUT]\n\
             ffbGain={:.2}\n\
             steerGamma={:.2}\n\
             steerDeadZone={:.3}\n\
             invertSteer={}\n",
            self.master_volume,
            hud_mode_name(self.hud_mode),
            camera_mode_name(self.camera_mode),
            self.ffb_gain,
            self.steer_gamma,
            self.steer_dead_zone,
            u8::from(self.invert_steer),
        );
        std::fs::write(path, text).is_ok()
    }

    /// Reads settings over the current values; a missing file is `false` and
    /// leaves them untouched, as with the other loaders in the crate.
    pub fn load(&mut self, path: impl AsRef<Path>) -> bool {
        let Ok(text) = std::fs::read_to_string(path.as_ref()) else {
            return false;
        };
        let ini = Ini::parse(&text);
        if let Some(v) = ini.get_f32("AUDIO", "masterVolume") {
            self.master_volume = v;
        }
        if let Some(v) = ini.get_f32("INPUT", "ffbGain") {
            self.ffb_gain = v;
        }
        if let Some(v) = ini.get_f32("INPUT", "steerGamma") {
            self.steer_gamma = f64::from(v);
        }
        if let Some(v) = ini.get_f32("INPUT", "steerDeadZone") {
            self.steer_dead_zone = f64::from(v);
        }
        if let Some(v) = ini
            .section("INPUT")
            .and_then(|section| section.get_bool("invertSteer"))
        {
            self.invert_steer = v;
        }
        if let Some(v) = ini.get("GRAPHICS", "hudMode") {
            if let Some(mode) = parse_hud_mode(v) {
                self.hud_mode = mode;
            }
        }
        if let Some(v) = ini.get("GRAPHICS", "camera") {
            if let Some(mode) = parse_camera_mode(v) {
                self.camera_mode = mode;
            }
        }
        self.clamp();
        true
    }
}

/// One actionable row of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuItem {
    /// Close the menu and drive.
    Resume,
    /// Open the settings screen.
    Settings,
    /// Open the key-rebind screen.
    KeyBindings,
    /// Reset the car to the grid.
    RestartSession,
    /// Leave the game.
    Quit,
    /// A settings row.
    Setting(SettingField),
    /// A rebindable action.
    Binding(BindingAction),
    /// Return to the previous screen.
    Back,
}

/// What activating a row asked the app layer to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    /// The menu closed: resume the simulation.
    Resume,
    /// Reset the car (the menu already closed).
    RestartSession,
    /// Quit the process.
    Quit,
    /// A setting changed and must be applied to the app.
    SettingsChanged(SettingField),
    /// Wait for the next key press to bind `action`.
    Rebind(BindingAction),
}

/// A drawn row: what it does, its label and its current value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuEntry {
    /// The action the row performs.
    pub item: MenuItem,
    /// Left-hand label.
    pub label: String,
    /// Right-hand value, when the row has one.
    pub value: Option<String>,
}

/// The menu model: current screen, cursor, settings and key map.
#[derive(Clone, Debug)]
pub struct GameMenu {
    screen: MenuScreen,
    cursor: usize,
    settings: MenuSettings,
    keyboard: KeyboardMapping,
    rebinding: Option<BindingAction>,
    status: String,
}

impl Default for GameMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl GameMenu {
    /// A closed menu with default settings and key bindings.
    pub fn new() -> Self {
        Self {
            screen: MenuScreen::Closed,
            cursor: 0,
            settings: MenuSettings::default(),
            keyboard: KeyboardMapping::default(),
            rebinding: None,
            status: String::new(),
        }
    }

    /// The current screen.
    pub fn screen(&self) -> MenuScreen {
        self.screen
    }

    /// `true` while a screen is shown (and the simulation must not step).
    pub fn is_open(&self) -> bool {
        self.screen != MenuScreen::Closed
    }

    /// `true` while the menu holds the simulation.
    pub fn is_paused(&self) -> bool {
        self.is_open()
    }

    /// The settings being edited.
    pub fn settings(&self) -> &MenuSettings {
        &self.settings
    }

    /// The settings being edited, mutably.
    pub fn settings_mut(&mut self) -> &mut MenuSettings {
        &mut self.settings
    }

    /// The key map being edited.
    pub fn keyboard(&self) -> &KeyboardMapping {
        &self.keyboard
    }

    /// The key map being edited, mutably.
    pub fn keyboard_mut(&mut self) -> &mut KeyboardMapping {
        &mut self.keyboard
    }

    /// The last status line (a rebind confirmation, an unsupported key, …).
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Replaces the status line.
    pub fn set_status(&mut self, text: impl Into<String>) {
        self.status = text.into();
    }

    /// The action waiting for a key press, if any.
    pub fn rebinding(&self) -> Option<BindingAction> {
        self.rebinding
    }

    /// The cursor position.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Opens the pause screen.
    pub fn open_pause(&mut self) {
        self.open(MenuScreen::Pause);
    }

    /// Opens `screen` with the cursor on the first row.
    pub fn open(&mut self, screen: MenuScreen) {
        self.screen = screen;
        self.cursor = 0;
        self.rebinding = None;
    }

    /// Closes the menu and drops any pending rebind.
    pub fn close(&mut self) {
        self.screen = MenuScreen::Closed;
        self.cursor = 0;
        self.rebinding = None;
    }

    /// The rows of the current screen, in draw order.
    pub fn entries(&self) -> Vec<MenuEntry> {
        let plain = |item, label: &str| MenuEntry {
            item,
            label: label.to_string(),
            value: None,
        };
        match self.screen {
            MenuScreen::Closed => Vec::new(),
            MenuScreen::Pause => vec![
                plain(MenuItem::Resume, "Resume"),
                plain(MenuItem::Settings, "Settings"),
                plain(MenuItem::KeyBindings, "Key bindings"),
                plain(MenuItem::RestartSession, "Restart session"),
                plain(MenuItem::Quit, "Quit"),
            ],
            MenuScreen::Settings => {
                let mut rows: Vec<MenuEntry> = SettingField::ALL
                    .iter()
                    .map(|field| MenuEntry {
                        item: MenuItem::Setting(*field),
                        label: field.label().to_string(),
                        value: Some(self.settings.value_label(*field)),
                    })
                    .collect();
                rows.push(plain(MenuItem::Back, "Back"));
                rows
            }
            MenuScreen::KeyRebind => {
                let mut rows: Vec<MenuEntry> = BindingAction::ALL
                    .iter()
                    .map(|action| MenuEntry {
                        item: MenuItem::Binding(*action),
                        label: action.label().to_string(),
                        value: Some(match action.key(&self.keyboard) {
                            0 => "-".to_string(),
                            vk => key_name(vk),
                        }),
                    })
                    .collect();
                rows.push(plain(MenuItem::Back, "Back"));
                rows
            }
        }
    }

    /// The row under the cursor.
    pub fn selected(&self) -> Option<MenuItem> {
        self.entries().get(self.cursor).map(|entry| entry.item)
    }

    /// Moves the cursor by `delta`, wrapping at both ends.
    pub fn move_cursor(&mut self, delta: i32) {
        let len = self.entries().len();
        if len == 0 {
            self.cursor = 0;
            return;
        }
        self.cursor = (self.cursor as i32 + delta).rem_euclid(len as i32) as usize;
    }

    /// Steps the row under the cursor (`Left`/`Right`). Returns the row when it
    /// was a setting and therefore changed.
    pub fn adjust(&mut self, dir: i32) -> Option<MenuItem> {
        let item = self.selected()?;
        if let MenuItem::Setting(field) = item {
            self.settings.adjust(field, dir);
            return Some(item);
        }
        None
    }

    /// Activates the row under the cursor (`Enter`).
    ///
    /// Navigation between screens happens here; everything the app layer must
    /// act on is returned.
    pub fn activate(&mut self) -> Option<MenuAction> {
        match self.selected()? {
            MenuItem::Resume => {
                self.close();
                Some(MenuAction::Resume)
            }
            MenuItem::Settings => {
                self.open(MenuScreen::Settings);
                None
            }
            MenuItem::KeyBindings => {
                self.open(MenuScreen::KeyRebind);
                None
            }
            MenuItem::RestartSession => {
                self.close();
                Some(MenuAction::RestartSession)
            }
            MenuItem::Quit => {
                self.close();
                Some(MenuAction::Quit)
            }
            MenuItem::Setting(field) => {
                self.settings.adjust(field, 1);
                Some(MenuAction::SettingsChanged(field))
            }
            MenuItem::Binding(action) => {
                self.begin_rebind(action);
                Some(MenuAction::Rebind(action))
            }
            MenuItem::Back => self.back(),
        }
    }

    /// `Esc`/`Back`: leaves a sub-screen, or closes the pause screen.
    pub fn back(&mut self) -> Option<MenuAction> {
        match self.screen {
            MenuScreen::Closed => None,
            MenuScreen::Pause => {
                self.close();
                Some(MenuAction::Resume)
            }
            MenuScreen::Settings | MenuScreen::KeyRebind => {
                let status = std::mem::take(&mut self.status);
                self.open(MenuScreen::Pause);
                self.status = status;
                None
            }
        }
    }

    /// Starts waiting for the key that will be bound to `action`.
    pub fn begin_rebind(&mut self, action: BindingAction) {
        self.rebinding = Some(action);
        self.status = format!("Press a key for {} (Esc cancels)", action.label());
    }

    /// Cancels a pending rebind.
    pub fn cancel_rebind(&mut self) {
        if self.rebinding.take().is_some() {
            self.status = "Rebind cancelled".to_string();
        }
    }

    /// Binds the pending action to `vk`.
    ///
    /// A key already bound to another action is swapped rather than dropped, so
    /// no action is ever left without a key by a rebind. Returns the action
    /// that was bound, or `None` when no rebind was pending or `vk` is not a
    /// usable code.
    pub fn capture_key(&mut self, vk: i32) -> Option<BindingAction> {
        let action = self.rebinding?;
        if !(1..=255).contains(&vk) {
            self.status = format!("Unsupported key (VK{vk})");
            return None;
        }
        self.rebinding = None;

        let previous = action.key(&self.keyboard);
        if let Some(other) = BindingAction::ALL
            .iter()
            .find(|other| **other != action && other.key(&self.keyboard) == vk)
        {
            other.set(&mut self.keyboard, previous);
        }
        action.set(&mut self.keyboard, vk);
        self.status = format!("{} = {}", action.label(), key_name(vk));
        Some(action)
    }

    /// Persists the settings ([`SETTINGS_PATH`] by default in the app layer).
    pub fn save_settings(&self, path: impl AsRef<Path>) -> bool {
        self.settings.save(path)
    }

    /// Loads settings over the current ones.
    pub fn load_settings(&mut self, path: impl AsRef<Path>) -> bool {
        self.settings.load(path)
    }

    /// Persists the key map in the shared flat-INI format.
    pub fn save_keyboard(&self, path: impl AsRef<Path>) -> bool {
        save_keyboard_mapping(path.as_ref(), &self.keyboard)
    }

    /// Loads the key map over the current bindings.
    pub fn load_keyboard(&mut self, path: impl AsRef<Path>) -> bool {
        load_keyboard_mapping(path.as_ref(), &mut self.keyboard)
    }
}

fn cycle_hud_mode(mode: RaceHudMode, forward: bool) -> RaceHudMode {
    if forward {
        mode.next()
    } else {
        mode.next().next().next()
    }
}

fn cycle_camera_mode(mode: CameraMode, forward: bool) -> CameraMode {
    let next = |mode: CameraMode| match mode {
        CameraMode::Cockpit => CameraMode::Chase,
        CameraMode::Chase => CameraMode::Free,
        CameraMode::Free => CameraMode::Cockpit,
    };
    if forward {
        next(mode)
    } else {
        // Three camera modes, so two steps forward is one step back.
        next(next(mode))
    }
}

/// Stable name written to `settings.ini` for a HUD mode.
pub fn hud_mode_name(mode: RaceHudMode) -> &'static str {
    match mode {
        RaceHudMode::Off => "Off",
        RaceHudMode::Compact => "Compact",
        RaceHudMode::Race => "Race",
        RaceHudMode::Engineer => "Engineer",
    }
}

/// Parses [`hud_mode_name`] back, case-insensitively.
pub fn parse_hud_mode(name: &str) -> Option<RaceHudMode> {
    match name.trim().to_ascii_lowercase().as_str() {
        "off" => Some(RaceHudMode::Off),
        "compact" => Some(RaceHudMode::Compact),
        "race" => Some(RaceHudMode::Race),
        "engineer" => Some(RaceHudMode::Engineer),
        _ => None,
    }
}

/// Stable name written to `settings.ini` for a camera mode.
pub fn camera_mode_name(mode: CameraMode) -> &'static str {
    match mode {
        CameraMode::Cockpit => "Cockpit",
        CameraMode::Chase => "Chase",
        CameraMode::Free => "Free",
    }
}

/// Parses [`camera_mode_name`] back, case-insensitively.
pub fn parse_camera_mode(name: &str) -> Option<CameraMode> {
    match name.trim().to_ascii_lowercase().as_str() {
        "cockpit" => Some(CameraMode::Cockpit),
        "chase" => Some(CameraMode::Chase),
        "free" => Some(CameraMode::Free),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("racingedge-menu-{tag}-{}", std::process::id()))
    }

    #[test]
    fn pause_screen_lists_the_app_actions() {
        let mut menu = GameMenu::new();
        assert!(!menu.is_open());
        assert!(menu.entries().is_empty());

        menu.open_pause();
        let labels: Vec<String> = menu.entries().into_iter().map(|e| e.label).collect();
        assert_eq!(
            labels,
            [
                "Resume",
                "Settings",
                "Key bindings",
                "Restart session",
                "Quit"
            ]
        );
        assert_eq!(menu.screen().title(), "PAUSED");
        assert!(menu.is_paused());
    }

    #[test]
    fn cursor_wraps_in_both_directions() {
        let mut menu = GameMenu::new();
        menu.open_pause();
        menu.move_cursor(-1);
        assert_eq!(menu.cursor(), 4, "up from the first row wraps to the last");
        menu.move_cursor(1);
        assert_eq!(menu.cursor(), 0);
        menu.move_cursor(2);
        assert_eq!(menu.cursor(), 2);
    }

    #[test]
    fn resume_and_back_close_the_menu() {
        let mut menu = GameMenu::new();
        menu.open_pause();
        assert_eq!(menu.activate(), Some(MenuAction::Resume));
        assert!(!menu.is_open());

        menu.open_pause();
        assert_eq!(menu.back(), Some(MenuAction::Resume));
        assert!(!menu.is_open());
    }

    #[test]
    fn settings_screen_opens_and_back_returns_to_pause() {
        let mut menu = GameMenu::new();
        menu.open_pause();
        menu.move_cursor(1);
        assert_eq!(
            menu.activate(),
            None,
            "opening a screen is not an app action"
        );
        assert_eq!(menu.screen(), MenuScreen::Settings);
        assert_eq!(menu.entries().len(), SettingField::ALL.len() + 1);
        assert_eq!(menu.entries().last().expect("back").item, MenuItem::Back);

        assert_eq!(menu.back(), None);
        assert_eq!(menu.screen(), MenuScreen::Pause);
        assert!(menu.is_open());
    }

    #[test]
    fn settings_clamp_at_their_limits() {
        let mut settings = MenuSettings::default();
        for _ in 0..50 {
            settings.adjust(SettingField::MasterVolume, 1);
            settings.adjust(SettingField::FfbGain, 1);
            settings.adjust(SettingField::SteerGamma, 1);
            settings.adjust(SettingField::SteerDeadZone, -1);
        }
        assert_eq!(settings.master_volume, 1.0);
        assert_eq!(settings.ffb_gain, 1.0);
        assert_eq!(settings.steer_gamma, 2.0);
        assert_eq!(settings.steer_dead_zone, 0.0);

        for _ in 0..50 {
            settings.adjust(SettingField::MasterVolume, -1);
            settings.adjust(SettingField::SteerGamma, -1);
            settings.adjust(SettingField::SteerDeadZone, 1);
        }
        assert_eq!(settings.master_volume, 0.0);
        assert_eq!(settings.steer_gamma, 0.5);
        assert_eq!(settings.steer_dead_zone, 0.4);
    }

    #[test]
    fn modes_cycle_both_ways() {
        let mut settings = MenuSettings::default();
        settings.adjust(SettingField::HudMode, 1);
        assert_eq!(settings.hud_mode, RaceHudMode::Engineer);
        settings.adjust(SettingField::HudMode, -1);
        assert_eq!(settings.hud_mode, RaceHudMode::Race);
        settings.adjust(SettingField::Camera, 1);
        assert_eq!(settings.camera_mode, CameraMode::Free);
        settings.adjust(SettingField::Camera, -1);
        assert_eq!(settings.camera_mode, CameraMode::Chase);
        settings.adjust(SettingField::InvertSteer, 1);
        assert!(settings.invert_steer);
        assert_eq!(settings.value_label(SettingField::InvertSteer), "on");
    }

    #[test]
    fn enter_on_a_setting_row_cycles_its_value() {
        let mut menu = GameMenu::new();
        menu.open(MenuScreen::Settings);
        let before = menu.settings().master_volume;
        assert_eq!(
            menu.activate(),
            Some(MenuAction::SettingsChanged(SettingField::MasterVolume))
        );
        assert!(menu.settings().master_volume > before);
        assert_eq!(menu.screen(), MenuScreen::Settings, "the screen stays open");
    }

    #[test]
    fn arrows_on_the_settings_row_adjust_it() {
        let mut menu = GameMenu::new();
        menu.open(MenuScreen::Settings);
        menu.move_cursor(1);
        assert_eq!(
            menu.adjust(1),
            Some(MenuItem::Setting(SettingField::HudMode))
        );
        assert_eq!(menu.settings().hud_mode, RaceHudMode::Engineer);
        menu.move_cursor(6);
        assert_eq!(menu.selected(), Some(MenuItem::Back));
        assert_eq!(
            menu.activate(),
            None,
            "Back is navigation, not an app action"
        );
        assert_eq!(menu.screen(), MenuScreen::Pause);
    }

    #[test]
    fn rebinding_swaps_a_key_instead_of_dropping_it() {
        let mut menu = GameMenu::new();
        menu.open(MenuScreen::KeyRebind);
        assert_eq!(
            menu.selected(),
            Some(MenuItem::Binding(BindingAction::Throttle))
        );

        assert_eq!(
            menu.activate(),
            Some(MenuAction::Rebind(BindingAction::Throttle))
        );
        assert_eq!(menu.rebinding(), Some(BindingAction::Throttle));

        // "S" is the brake key: the swap must leave the brake on the old W.
        let bound = menu.capture_key(b'S' as i32);
        assert_eq!(bound, Some(BindingAction::Throttle));
        assert_eq!(menu.keyboard().throttle, b'S' as i32);
        assert_eq!(menu.keyboard().brake, b'W' as i32);
        assert_eq!(menu.rebinding(), None);
        assert!(menu.status().contains("Throttle"));
    }

    #[test]
    fn pressing_escape_cancels_a_pending_rebind() {
        let mut menu = GameMenu::new();
        menu.open(MenuScreen::KeyRebind);
        menu.begin_rebind(BindingAction::Handbrake);
        menu.cancel_rebind();
        assert_eq!(menu.rebinding(), None);
        assert_eq!(
            menu.keyboard().handbrake,
            KeyboardMapping::default().handbrake
        );
        assert!(menu.status().contains("cancelled"));
    }

    #[test]
    fn capture_without_a_pending_rebind_is_ignored() {
        let mut menu = GameMenu::new();
        assert_eq!(menu.capture_key(b'X' as i32), None);
        assert_eq!(menu.keyboard().throttle, b'W' as i32);
    }

    #[test]
    fn settings_round_trip_through_the_ini_file() {
        let dir = temp_dir("settings");
        let path = dir.join("settings.ini");
        let mut settings = MenuSettings {
            master_volume: 0.35,
            hud_mode: RaceHudMode::Engineer,
            camera_mode: CameraMode::Cockpit,
            ffb_gain: 0.6,
            steer_gamma: 1.4,
            steer_dead_zone: 0.12,
            invert_steer: true,
        };
        settings.clamp();
        assert!(settings.save(&path));

        let mut loaded = MenuSettings::default();
        assert!(loaded.load(&path));
        assert_eq!(loaded.hud_mode, settings.hud_mode);
        assert_eq!(loaded.camera_mode, settings.camera_mode);
        assert_eq!(loaded.invert_steer, settings.invert_steer);
        assert_eq!(loaded.master_volume, settings.master_volume);
        assert_eq!(loaded.ffb_gain, settings.ffb_gain);
        // The shared INI reader yields `f32`, so the `f64` curve settings come
        // back to within one `f32` ulp of what was written.
        assert!((loaded.steer_gamma - settings.steer_gamma).abs() < 1e-3);
        assert!((loaded.steer_dead_zone - settings.steer_dead_zone).abs() < 1e-5);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_settings_file_keeps_the_defaults() {
        let path = temp_dir("absent").join("settings.ini");
        let mut settings = MenuSettings::default();
        assert!(!settings.load(&path));
        assert_eq!(settings, MenuSettings::default());
    }

    #[test]
    fn keyboard_round_trips_through_the_shared_loader() {
        let dir = temp_dir("keys");
        let path = dir.join("keyboard.ini");
        let mut menu = GameMenu::new();
        menu.keyboard_mut().throttle = b'I' as i32;
        menu.keyboard_mut().handbrake = b' ' as i32;
        assert!(menu.save_keyboard(&path));

        let mut loaded = GameMenu::new();
        assert!(loaded.load_keyboard(&path));
        assert_eq!(loaded.keyboard().throttle, b'I' as i32);
        assert_eq!(loaded.keyboard().handbrake, b' ' as i32);
        assert_eq!(loaded.keyboard().brake, b'S' as i32);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn settings_names_parse_case_insensitively() {
        assert_eq!(parse_hud_mode("engineer"), Some(RaceHudMode::Engineer));
        assert_eq!(parse_hud_mode("nope"), None);
        assert_eq!(parse_camera_mode(" COCKPIT "), Some(CameraMode::Cockpit));
        assert_eq!(parse_camera_mode("nope"), None);
        assert_eq!(hud_mode_name(RaceHudMode::Compact), "Compact");
        assert_eq!(camera_mode_name(CameraMode::Free), "Free");
    }

    #[test]
    fn deprecated_or_closed_screens_have_no_rows() {
        let mut menu = GameMenu::new();
        menu.open(MenuScreen::Pause);
        menu.close();
        assert_eq!(menu.screen(), MenuScreen::Closed);
        assert!(menu.entries().is_empty());
        assert_eq!(menu.selected(), None);
        assert_eq!(menu.activate(), None);
        assert_eq!(menu.back(), None);
    }
}
