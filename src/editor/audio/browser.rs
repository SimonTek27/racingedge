//! In-game bank browser.
//!
//! A small Bevy UI panel that lists every bank loaded into the
//! [`BankLibrary`] and lets the user preview samples with a click. The panel is
//! rebuilt reactively whenever the library changes, so it does not depend on
//! plugin or system ordering.
//!
//! Controls:
//! * `F1` — show/hide the panel.
//! * Click a sample — play it (looping samples loop).

use bevy::prelude::*;

use crate::editor::audio::plugin::{play_sample, BankLibrary, BankSampleSummary, LoadedBank};

/// Marker for the browser panel root (used for show/hide toggling).
#[derive(Component)]
pub struct BankBrowserRoot;

/// Marker for the node that holds the generated sample rows.
#[derive(Component)]
pub struct SampleList;

/// A clickable row that plays `bank`'s `sample`.
#[derive(Component)]
pub struct SampleRow {
    /// Index into [`BankLibrary::banks`].
    pub bank: usize,
    /// Sample index within that bank.
    pub sample: usize,
}

/// The panel's normal background colour.
const ROW_IDLE: BackgroundColor = BackgroundColor(Color::rgba(1.0, 1.0, 1.0, 0.05));
/// The panel's hovered background colour.
const ROW_HOVERED: BackgroundColor = BackgroundColor(Color::rgba(1.0, 1.0, 1.0, 0.18));
/// The panel's pressed background colour.
const ROW_PRESSED: BackgroundColor = BackgroundColor(Color::rgba(0.30, 0.60, 1.0, 0.60));

/// One-line label describing a sample row.
pub fn sample_row_label(summary: &BankSampleSummary) -> String {
    let looped = if summary.looped { "  loop" } else { "" };
    let codec = summary.playable_as.unwrap_or("unsupported");
    format!(
        "{:<20} {:>6} Hz  {} ch  {:>6.2}s{looped}  [{codec}]",
        summary.name, summary.frequency, summary.channels, summary.duration_seconds,
    )
}

/// Heading text for a bank.
pub fn bank_header(index: usize, bank: &LoadedBank) -> String {
    format!(
        "[{index}] {}  ({}, {} sample(s))",
        bank.path,
        bank.format,
        bank.summaries.len()
    )
}

/// Plugin that adds the bank browser panel.
pub struct BankBrowserPlugin;

impl Plugin for BankBrowserPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_bank_browser).add_systems(
            Update,
            (
                rebuild_sample_list,
                update_row_colors,
                handle_sample_clicks,
                toggle_bank_browser,
            ),
        );
    }
}

/// Spawn a UI camera (if the game has none) and the empty panel skeleton.
fn setup_bank_browser(mut commands: Commands, cameras: Query<(), With<Camera>>) {
    if cameras.is_empty() {
        commands.spawn(Camera2dBundle::default());
    }

    commands
        .spawn((
            NodeBundle {
                style: Style {
                    position_type: PositionType::Absolute,
                    left: Val::Px(12.0),
                    top: Val::Px(12.0),
                    width: Val::Px(520.0),
                    max_height: Val::Percent(88.0),
                    flex_direction: FlexDirection::Column,
                    row_gap: Val::Px(4.0),
                    padding: UiRect::all(Val::Px(10.0)),
                    ..default()
                },
                background_color: BackgroundColor(Color::rgba(0.0, 0.0, 0.0, 0.72)),
                ..default()
            },
            BankBrowserRoot,
        ))
        .with_children(|root| {
            root.spawn(TextBundle::from_section(
                "Bank Browser   (F1 to toggle, click a sample to play)",
                TextStyle {
                    font_size: 15.0,
                    color: Color::rgb(0.95, 0.95, 0.95),
                    ..default()
                },
            ));
            root.spawn((
                NodeBundle {
                    style: Style {
                        flex_direction: FlexDirection::Column,
                        row_gap: Val::Px(2.0),
                        ..default()
                    },
                    ..default()
                },
                SampleList,
            ));
        });
}

/// Rebuild the sample rows whenever the library changes (and once at startup).
fn rebuild_sample_list(
    mut commands: Commands,
    library: Res<BankLibrary>,
    list: Query<Entity, With<SampleList>>,
    mut initialized: Local<bool>,
) {
    if *initialized && !library.is_changed() {
        return;
    }
    *initialized = true;

    let Ok(list_entity) = list.get_single() else {
        return;
    };
    commands.entity(list_entity).despawn_descendants();

    if library.is_empty() {
        commands.entity(list_entity).with_children(|list| {
            list.spawn(TextBundle::from_section(
                "No banks loaded. Set RACINGEDGE_BANKS or add .fsb files to assets/audio/.",
                TextStyle {
                    font_size: 12.0,
                    color: Color::rgb(0.75, 0.75, 0.75),
                    ..default()
                },
            ));
        });
        return;
    }

    for (bank_index, bank) in library.banks.iter().enumerate() {
        let header = bank_header(bank_index, bank);
        commands.entity(list_entity).with_children(|list| {
            list.spawn(TextBundle::from_section(
                header,
                TextStyle {
                    font_size: 13.0,
                    color: Color::rgb(0.80, 0.86, 1.0),
                    ..default()
                },
            ));
        });

        for (sample_index, summary) in bank.summaries.iter().enumerate() {
            let label = sample_row_label(summary);
            let row = SampleRow {
                bank: bank_index,
                sample: sample_index,
            };
            commands.entity(list_entity).with_children(|list| {
                list.spawn((
                    ButtonBundle {
                        style: Style {
                            padding: UiRect::axes(Val::Px(6.0), Val::Px(3.0)),
                            ..default()
                        },
                        background_color: ROW_IDLE,
                        ..default()
                    },
                    row,
                ))
                .with_children(|button| {
                    button.spawn(TextBundle::from_section(
                        label,
                        TextStyle {
                            font_size: 12.0,
                            color: Color::rgb(0.88, 0.88, 0.88),
                            ..default()
                        },
                    ));
                });
            });
        }
    }
}

/// Query delle righe del browser da ricolorare in base all'interazione.
type RowHighlightQuery<'w, 's> = Query<
    'w,
    's,
    (&'static Interaction, &'static mut BackgroundColor),
    (Changed<Interaction>, With<SampleRow>),
>;

/// Highlight rows on hover/press.
fn update_row_colors(mut rows: RowHighlightQuery) {
    for (interaction, mut color) in &mut rows {
        *color = match interaction {
            Interaction::Pressed => ROW_PRESSED,
            Interaction::Hovered => ROW_HOVERED,
            Interaction::None => ROW_IDLE,
        };
    }
}

/// Play a sample when its row is clicked.
fn handle_sample_clicks(
    rows: Query<(&Interaction, &SampleRow), Changed<Interaction>>,
    library: Res<BankLibrary>,
    mut commands: Commands,
) {
    for (interaction, row) in &rows {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let looped = library
            .banks
            .get(row.bank)
            .and_then(|bank| bank.summaries.get(row.sample))
            .map(|summary| summary.looped)
            .unwrap_or(false);
        let _ = play_sample(&mut commands, &library, row.bank, row.sample, looped);
    }
}

/// Toggle the panel with `F1`.
fn toggle_bank_browser(
    keys: Res<Input<KeyCode>>,
    mut roots: Query<&mut Style, With<BankBrowserRoot>>,
) {
    if !keys.just_pressed(KeyCode::F1) {
        return;
    }
    for mut style in &mut roots {
        style.display = if style.display == bevy::ui::Display::None {
            bevy::ui::Display::Flex
        } else {
            bevy::ui::Display::None
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::audio::fsb::SoundFormat;

    fn summary(name: &str, looped: bool) -> BankSampleSummary {
        BankSampleSummary {
            index: 0,
            name: name.to_string(),
            frequency: 44_100,
            channels: 2,
            num_frames: 4_410,
            duration_seconds: 0.1,
            looped,
            bytes: 17_640,
            playable_as: Some("wav"),
        }
    }

    #[test]
    fn label_marks_loops_and_codec() {
        let label = sample_row_label(&summary("engine", true));
        assert!(label.contains("engine"), "{label}");
        assert!(label.contains("44100 Hz"), "{label}");
        assert!(label.contains("loop"), "{label}");
        assert!(label.contains("[wav]"), "{label}");

        let mut unsupported = summary("movie", false);
        unsupported.playable_as = None;
        assert!(sample_row_label(&unsupported).contains("[unsupported]"));
    }

    #[test]
    fn header_reports_bank_and_count() {
        let bank = LoadedBank {
            path: "engine.fsb".to_string(),
            format: SoundFormat::Vorbis,
            summaries: vec![summary("idle", false)],
            handles: vec![None],
        };
        let header = bank_header(3, &bank);
        assert!(header.contains("[3]"));
        assert!(header.contains("engine.fsb"));
        assert!(header.contains("vorbis"));
        assert!(header.contains("1 sample(s)"));
    }

    #[test]
    fn browser_rows_track_the_library() {
        let mut app = App::new();
        app.init_resource::<BankLibrary>();
        app.world.spawn(SampleList);
        app.add_systems(Update, rebuild_sample_list);

        // First frame: empty library -> placeholder, no rows.
        app.update();
        assert_eq!(count_rows(&mut app), 0);

        // Add a bank with two samples: the next update rebuilds the list.
        {
            let mut library = app.world.resource_mut::<BankLibrary>();
            library.banks.push(LoadedBank {
                path: "engine.fsb".to_string(),
                format: SoundFormat::Pcm16,
                summaries: vec![summary("idle", false), summary("rev", true)],
                handles: vec![None, None],
            });
        }
        app.update();
        assert_eq!(count_rows(&mut app), 2);
    }

    fn count_rows(app: &mut App) -> usize {
        app.world.query::<&SampleRow>().iter(&app.world).count()
    }
}
