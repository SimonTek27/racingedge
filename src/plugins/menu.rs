//! The Bevy side of the menus: the pause / settings / key-rebind screen, the
//! pause gate for the simulation, and the bridge from the shared
//! [`KeyboardMapping`](crate::simulator::input::KeyboardMapping) to the drive
//! loop's [`DriveBindings`].
//!
//! The model — screens, rows, cursor, settings and rebind rules — lives in
//! [`crate::simulator::menus`] and is unit-tested there. This module only turns
//! key presses into model calls, applies the result to the app's resources, and
//! renders the rows as Bevy UI text, the same way [`crate::simulator::hud`]
//! renders the race HUD.
//!
//! Master volume is the one setting that belongs to a mixer: it is pushed into
//! Bevy's `GlobalVolume` under `game-audio`, and left in [`MenuState`] for a host
//! that brings its own mixer (Renzora's `renzora_audio`) to read.

#[cfg(feature = "game-audio")]
use bevy::audio::{GlobalVolume, Volume};
use bevy::prelude::*;

use super::game::DriveCamera;
use super::keymap::vk_from_key_code;
use super::physics::DriveBindings;
use super::SimulationPause;
use crate::simulator::hud::HudState;
use crate::simulator::menus::{
    GameMenu, MenuAction, MenuItem, SettingField, KEYBOARD_PATH, SETTINGS_PATH,
};
use crate::simulator::physics::VehicleState;

/// The menu model as a resource.
#[derive(Resource, Default)]
pub struct MenuState(pub GameMenu);

/// Marker on the overlay root.
#[derive(Component)]
pub struct MenuRoot;

/// The overlay's single text node: title, rows and hint in one block.
#[derive(Component)]
pub struct MenuPanel;

/// The hint shown when the menu has nothing else to report.
pub const MENU_HINT: &str = "Up/Down move   Left/Right change   Enter select   Esc back";

/// Plugin: loads the persisted settings/key map, draws the overlay and keeps
/// [`SimulationPause`] in step with the menu.
pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MenuState>()
            .init_resource::<SimulationPause>()
            // The menu applies its settings to these, so make sure they exist
            // even when the plugin is added on its own (a host without
            // `GamePlugin`). `init_resource` is idempotent.
            .init_resource::<HudState>()
            .init_resource::<DriveCamera>()
            .add_systems(Startup, (load_persisted, spawn_menu_overlay))
            .add_systems(
                Update,
                (handle_menu_keys, sync_pause, sync_bindings, render_menu).chain(),
            );

        // Bevy's mixer exists only when this crate (or the host) enables
        // `bevy_audio`; without it the setting stays readable in `MenuState`.
        #[cfg(feature = "game-audio")]
        app.init_resource::<GlobalVolume>()
            .add_systems(Update, sync_master_volume);
    }
}

/// Reads `user/settings.ini` and `user/keyboard.ini` into the menu, and pushes
/// the bindings to the drive loop. Missing files leave the defaults in place.
fn load_persisted(mut menu: ResMut<MenuState>, mut bindings: ResMut<DriveBindings>) {
    menu.0.load_settings(SETTINGS_PATH);
    menu.0.load_keyboard(KEYBOARD_PATH);
    bindings.0 = *menu.0.keyboard();
}

/// Applies the settings the menu edits to the resources that own them.
///
/// Master volume is not here: it is the mixer's, and the mixer may be the
/// host's ([`sync_master_volume`] handles Bevy's).
fn apply_settings(
    field: SettingField,
    menu: &GameMenu,
    hud: &mut HudState,
    camera: &mut DriveCamera,
) {
    let settings = menu.settings();
    match field {
        SettingField::HudMode => hud.hud.set_mode(settings.hud_mode),
        SettingField::Camera => camera.0.set_mode(settings.camera_mode),
        // Master volume belongs to the mixer, and the remaining rows to whoever
        // owns the device: the FFB bridge, the input manager's curve and the
        // steering axis.
        SettingField::MasterVolume
        | SettingField::FfbGain
        | SettingField::SteerGamma
        | SettingField::SteerDeadZone
        | SettingField::InvertSteer => {}
    }
}

fn reset_cars(cars: &mut Query<(&mut VehicleState, &mut Transform)>) {
    for (mut car, mut transform) in cars.iter_mut() {
        *car = VehicleState::default();
        *transform = Transform::from_xyz(0.0, 0.55, 0.0);
    }
}

/// Keyboard → model: pause toggle, cursor movement, value changes, activation
/// and key capture.
fn handle_menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut menu: ResMut<MenuState>,
    mut hud: ResMut<HudState>,
    mut camera: ResMut<DriveCamera>,
    mut exit: MessageWriter<AppExit>,
    mut cars: Query<(&mut VehicleState, &mut Transform)>,
) {
    if !menu.0.is_open() {
        if keys.just_pressed(KeyCode::Escape) {
            menu.0.open_pause();
        }
        return;
    }

    // While a rebind waits for a key, the next supported key is the binding.
    if menu.0.rebinding().is_some() {
        if keys.just_pressed(KeyCode::Escape) {
            menu.0.cancel_rebind();
            return;
        }
        let Some(code) = keys.get_just_pressed().next().copied() else {
            return;
        };
        match vk_from_key_code(code) {
            Some(vk) => {
                if menu.0.capture_key(vk).is_some() {
                    menu.0.save_keyboard(KEYBOARD_PATH);
                }
            }
            None => menu.0.set_status(format!("{code:?} cannot be bound")),
        }
        return;
    }

    if keys.just_pressed(KeyCode::Escape) {
        // Closing the pause screen is `Resume`; the pause flag follows in
        // `sync_pause`, which also drops any held input.
        let _ = menu.0.back();
        return;
    }

    if keys.just_pressed(KeyCode::ArrowUp) {
        menu.0.move_cursor(-1);
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        menu.0.move_cursor(1);
    }

    let mut changed = None;
    if keys.just_pressed(KeyCode::ArrowLeft) {
        if let Some(MenuItem::Setting(field)) = menu.0.adjust(-1) {
            changed = Some(field);
        }
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        if let Some(MenuItem::Setting(field)) = menu.0.adjust(1) {
            changed = Some(field);
        }
    }

    if keys.just_pressed(KeyCode::Enter) {
        match menu.0.activate() {
            Some(MenuAction::SettingsChanged(field)) => changed = Some(field),
            Some(MenuAction::RestartSession) => reset_cars(&mut cars),
            Some(MenuAction::Quit) => {
                exit.write(AppExit::Success);
            }
            Some(MenuAction::Resume) | Some(MenuAction::Rebind(_)) | None => {}
        }
    }

    if let Some(field) = changed {
        apply_settings(field, &menu.0, &mut hud, &mut camera);
        menu.0.save_settings(SETTINGS_PATH);
    }
}

/// Pushes the menu's master volume into Bevy's mixer.
///
/// Bevy's own volumes count as host-owned: a Renzora build has no `GlobalVolume`
/// and reads `MenuState::settings().master_volume` through its own mixer instead.
#[cfg(feature = "game-audio")]
fn sync_master_volume(menu: Res<MenuState>, mut volume: ResMut<GlobalVolume>) {
    if !menu.is_changed() {
        return;
    }
    volume.volume = Volume::Linear(menu.0.settings().master_volume);
}

/// Keeps [`SimulationPause`] in step with the menu and drops any held input, so
/// the car does not launch itself when the pause ends.
fn sync_pause(
    menu: Res<MenuState>,
    mut pause: ResMut<SimulationPause>,
    mut cars: Query<&mut VehicleState>,
) {
    let paused = menu.0.is_paused();
    if pause.0 == paused {
        return;
    }
    pause.0 = paused;
    for mut car in cars.iter_mut() {
        car.throttle = 0.0;
        car.brake = 0.0;
        car.steer_input = 0.0;
    }
}

/// Pushes a rebind into the drive loop as soon as the menu changes.
fn sync_bindings(menu: Res<MenuState>, mut bindings: ResMut<DriveBindings>) {
    if !menu.is_changed() {
        return;
    }
    let keyboard = *menu.0.keyboard();
    if bindings.0 != keyboard {
        bindings.0 = keyboard;
    }
}

/// The whole screen as one text block: title, an arrow-marked row list and the
/// hint/status line. The model decides the rows; this only lays them out.
pub fn menu_text(menu: &GameMenu) -> String {
    let mut out = format!("{}\n\n", menu.screen().title());
    for (index, entry) in menu.entries().iter().enumerate() {
        let marker = if index == menu.cursor() { "> " } else { "  " };
        match &entry.value {
            Some(value) => out.push_str(&format!("{marker}{:<20}{value}\n", entry.label)),
            None => out.push_str(&format!("{marker}{}\n", entry.label)),
        }
    }
    out.push('\n');
    out.push_str(match menu.status() {
        "" => MENU_HINT,
        status => status,
    });
    out
}

/// Draws the current screen: visibility and the text block.
fn render_menu(
    menu: Res<MenuState>,
    mut root: Query<&mut Visibility, With<MenuRoot>>,
    mut panel: Query<&mut Text, With<MenuPanel>>,
) {
    let open = menu.0.is_open();
    if let Ok(mut visibility) = root.single_mut() {
        *visibility = if open {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if !open {
        return;
    }
    if let Ok(mut text) = panel.single_mut() {
        **text = menu_text(&menu.0);
    }
}

/// Builds the overlay once: an absolute panel holding one hidden text node.
fn spawn_menu_overlay(mut commands: Commands) {
    commands
        .spawn((
            MenuRoot,
            Visibility::Hidden,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(18.0),
                top: Val::Percent(16.0),
                width: Val::Percent(64.0),
                height: Val::Percent(68.0),
                padding: UiRect::all(Val::Px(26.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.02, 0.04, 0.88)),
        ))
        .with_children(|root| {
            root.spawn((
                MenuPanel,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(22.0),
                    ..default()
                },
                TextColor(Color::srgb(0.88, 0.88, 0.92)),
            ));
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulator::menus::MenuScreen;

    #[test]
    fn the_panel_marks_the_cursor_and_lays_out_values() {
        let mut menu = GameMenu::new();
        menu.open_pause();
        menu.move_cursor(1);
        let text = menu_text(&menu);
        assert!(text.starts_with("PAUSED\n\n"), "{text}");
        assert!(text.contains("> Settings"), "{text}");
        assert!(text.contains("  Resume"), "{text}");
        assert!(text.ends_with(MENU_HINT), "{text}");

        menu.open(MenuScreen::Settings);
        let text = menu_text(&menu);
        assert!(text.contains("> Master volume"), "{text}");
        assert!(text.contains("80%"), "{text}");
    }

    #[test]
    fn a_rebind_status_replaces_the_hint() {
        let mut menu = GameMenu::new();
        menu.open(MenuScreen::KeyRebind);
        menu.begin_rebind(crate::simulator::menus::BindingAction::Throttle);
        let text = menu_text(&menu);
        assert!(text.contains("Press a key for Throttle"), "{text}");
        assert!(!text.ends_with(MENU_HINT));
    }
}
