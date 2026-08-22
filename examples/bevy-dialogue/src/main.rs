use std::error::Error;
use std::thread;
use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::input_focus::InputFocus;
use bevy::prelude::*;
use weave_bevy::prelude::*;

const BACKGROUND: Color = Color::srgb(0.035, 0.043, 0.075);
const PANEL: Color = Color::srgb(0.075, 0.086, 0.135);
const PANEL_ALT: Color = Color::srgb(0.105, 0.094, 0.155);
const BORDER: Color = Color::srgb(0.23, 0.255, 0.36);
const TEXT: Color = Color::srgb(0.94, 0.94, 0.98);
const MUTED: Color = Color::srgb(0.66, 0.69, 0.78);
const ACCENT: Color = Color::srgb(0.69, 0.48, 0.98);
const NORMAL_BUTTON: Color = Color::srgb(0.20, 0.16, 0.31);
const HOVERED_BUTTON: Color = Color::srgb(0.30, 0.23, 0.45);
const PRESSED_BUTTON: Color = Color::srgb(0.45, 0.31, 0.65);
const MAX_EVENT_LOG: usize = 32;
const MAX_SMOKE_FRAMES: usize = 2_000;

#[derive(Resource, Clone, Copy)]
struct RunMode {
    auto_advance: bool,
}

#[derive(Resource)]
struct GameViewModel {
    status: String,
    dialogue: String,
    choices: Vec<String>,
    patterns: Vec<String>,
    observable_state: String,
    events: Vec<String>,
    can_continue: bool,
    finished: bool,
    failure: Option<String>,
    pattern_count: usize,
    line_count: usize,
    largest_choice_set: usize,
    reload_count: usize,
}

impl Default for GameViewModel {
    fn default() -> Self {
        Self {
            status: "Loading dialogue.weave through Bevy's asset pipeline...".to_owned(),
            dialogue: "The story will begin when its asset is ready.".to_owned(),
            choices: Vec::new(),
            patterns: Vec::new(),
            observable_state: "Waiting for runtime state...".to_owned(),
            events: Vec::new(),
            can_continue: false,
            finished: false,
            failure: None,
            pattern_count: 0,
            line_count: 0,
            largest_choice_set: 0,
            reload_count: 0,
        }
    }
}

impl GameViewModel {
    fn record(&mut self, event: impl Into<String>) {
        if self.events.len() == MAX_EVENT_LOG {
            self.events.remove(0);
        }
        self.events.push(event.into());
    }
}

#[derive(Component)]
enum ViewText {
    Status,
    Dialogue,
    Pattern,
    ObservableState,
    EventLog,
}

#[derive(Component)]
struct ChoiceList;

#[derive(Component)]
struct ChoiceButton;

#[derive(Component)]
struct ContinueButton;

#[derive(Component, Clone, Copy)]
enum UiAction {
    Continue,
    Choose(usize),
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ExampleSet {
    ObserveState,
    Interface,
}

struct ExampleGamePlugin {
    show_ui: bool,
    auto_advance: bool,
}

impl ExampleGamePlugin {
    const fn interactive() -> Self {
        Self {
            show_ui: true,
            auto_advance: false,
        }
    }

    const fn smoke_test() -> Self {
        Self {
            show_ui: false,
            auto_advance: true,
        }
    }
}

impl Plugin for ExampleGamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GameViewModel>()
            .insert_resource(RunMode {
                auto_advance: self.auto_advance,
            })
            .configure_sets(
                Update,
                (ExampleSet::ObserveState, ExampleSet::Interface)
                    .chain()
                    .after(WeaveSet::SyncAsset),
            )
            .add_systems(
                Update,
                sync_observable_state.in_set(ExampleSet::ObserveState),
            )
            .add_observer(on_ready)
            .add_observer(on_reloaded)
            .add_observer(on_line)
            .add_observer(on_choices)
            .add_observer(on_pattern)
            .add_observer(on_ended)
            .add_observer(on_failed);

        if self.show_ui {
            app.init_resource::<InputFocus>()
                .insert_resource(ClearColor(BACKGROUND))
                .add_systems(Startup, setup_ui)
                .add_systems(
                    Update,
                    (button_interactions, sync_ui).in_set(ExampleSet::Interface),
                );
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let arguments = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments.as_slice() {
        [] => {
            run_interactive();
            Ok(())
        }
        [argument] if argument == "--smoke-test" => run_smoke_test().map_err(Into::into),
        _ => Err("usage: weave-example-bevy-dialogue [--smoke-test]".into()),
    }
}

fn asset_plugin(watch_for_changes: bool) -> AssetPlugin {
    AssetPlugin {
        file_path: format!("{}/assets", env!("CARGO_MANIFEST_DIR")),
        watch_for_changes_override: Some(watch_for_changes),
        ..default()
    }
}

fn run_interactive() {
    App::new()
        .add_plugins(DefaultPlugins.set(asset_plugin(true)).set(WindowPlugin {
            primary_window: Some(Window {
                title: "Weave - The Ember Forecast".to_owned(),
                resolution: (1_120, 720).into(),
                resizable: true,
                ..default()
            }),
            ..default()
        }))
        .add_plugins((WeavePlugin, ExampleGamePlugin::interactive()))
        .insert_resource(WeaveStory::new("dialogue.weave").with_seed(7))
        .run();
}

fn build_smoke_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        asset_plugin(false),
        WeavePlugin,
        ExampleGamePlugin::smoke_test(),
    ))
    .insert_resource(WeaveStory::new("dialogue.weave").with_seed(7));
    app
}

fn run_smoke_test() -> Result<(), String> {
    let mut app = build_smoke_app();
    drive_smoke_test(&mut app)?;
    let view = app.world().resource::<GameViewModel>();
    println!(
        "Bevy integration smoke passed: {} lines, {} ordered pattern draws, choices, state, end, and reload feedback",
        view.line_count, view.pattern_count
    );
    Ok(())
}

fn drive_smoke_test(app: &mut App) -> Result<(), String> {
    for _ in 0..MAX_SMOKE_FRAMES {
        app.update();
        let view = app.world().resource::<GameViewModel>();
        if let Some(failure) = &view.failure {
            return Err(failure.clone());
        }
        if view.finished {
            break;
        }
        thread::sleep(Duration::from_millis(1));
    }

    if !app.world().resource::<GameViewModel>().finished {
        return Err("example timed out while loading or advancing dialogue.weave".to_owned());
    }

    app.world_mut().trigger(StoryReloaded);
    validate_smoke_result(app.world().resource::<GameViewModel>())
}

fn validate_smoke_result(view: &GameViewModel) -> Result<(), String> {
    if let Some(failure) = &view.failure {
        return Err(format!("story emitted StoryFailed: {failure}"));
    }
    if view.pattern_count != 3 {
        return Err(format!(
            "expected three PatternDrawn events, observed {}",
            view.pattern_count
        ));
    }
    if view.line_count != 3 {
        return Err(format!(
            "expected three DeliverLine events, observed {}",
            view.line_count
        ));
    }
    if view.largest_choice_set != 2 {
        return Err(format!(
            "expected a two-choice boundary, observed {} choices",
            view.largest_choice_set
        ));
    }
    if !view.observable_state.contains("cups: 1") {
        return Err(format!(
            "observable state did not expose the selected branch: {}",
            view.observable_state
        ));
    }
    if view.reload_count != 1 {
        return Err(format!(
            "expected one StoryReloaded reaction, observed {}",
            view.reload_count
        ));
    }

    let expected_flow = [
        "ready",
        "pattern:omens:dawn:",
        "pattern:omens:noon:",
        "pattern:omens:dusk:",
        "line:",
        "line:",
        "choices:2",
        "line:",
        "ended",
        "reloaded",
    ];
    let mut cursor = 0;
    for expected in expected_flow {
        let Some(relative) = view.events[cursor..]
            .iter()
            .position(|event| event.starts_with(expected))
        else {
            return Err(format!(
                "event flow is missing {expected:?} after index {cursor}: {:?}",
                view.events
            ));
        };
        cursor += relative + 1;
    }
    Ok(())
}

fn on_ready(_: On<StoryReady>, mut view: ResMut<GameViewModel>, mut commands: Commands) {
    view.status = "StoryReady | compiled source loaded".to_owned();
    view.record("ready");
    commands.weave_continue();
}

fn on_reloaded(_: On<StoryReloaded>, mut view: ResMut<GameViewModel>) {
    view.reload_count += 1;
    view.status = "StoryReloaded | compatible runtime state restored when possible".to_owned();
    view.record("reloaded");
}

fn on_line(
    line: On<DeliverLine>,
    mode: Res<RunMode>,
    mut view: ResMut<GameViewModel>,
    mut commands: Commands,
) {
    view.line_count += 1;
    view.dialogue.clone_from(&line.text);
    view.choices.clear();
    view.can_continue = !mode.auto_advance;
    view.status = if mode.auto_advance {
        "DeliverLine | smoke mode advancing".to_owned()
    } else {
        "DeliverLine | continue when ready".to_owned()
    };
    view.record(format!("line:{}", line.text));
    if mode.auto_advance {
        commands.weave_continue();
    }
}

fn on_choices(
    choices: On<DeliverChoices>,
    mode: Res<RunMode>,
    mut view: ResMut<GameViewModel>,
    mut commands: Commands,
) {
    view.choices = choices.iter().map(|choice| choice.text.clone()).collect();
    view.largest_choice_set = view.largest_choice_set.max(view.choices.len());
    view.can_continue = false;
    view.status = "DeliverChoices | select a response".to_owned();
    view.record(format!("choices:{}", choices.len()));
    if mode.auto_advance && !choices.is_empty() {
        commands.weave_choose(0);
    }
}

fn on_pattern(draw: On<PatternDrawn>, mut view: ResMut<GameViewModel>) {
    let position = draw.position.as_deref().unwrap_or("single");
    let name = draw.name.as_deref().unwrap_or(draw.element.as_str());
    let meaning = draw
        .meaning
        .as_ref()
        .map_or_else(|| "no meaning".to_owned(), ToString::to_string);
    let reversed = if draw.reversed { " | reversed" } else { "" };
    view.pattern_count += 1;
    view.patterns.push(format!(
        "{}  |  {}  |  {}{}",
        position.to_uppercase(),
        name,
        meaning,
        reversed
    ));
    view.record(format!(
        "pattern:{}:{}:{}",
        draw.system, position, draw.element
    ));
}

fn on_ended(_: On<StoryEnded>, mut view: ResMut<GameViewModel>) {
    view.finished = true;
    view.can_continue = false;
    view.choices.clear();
    view.status = "StoryEnded | save dialogue.weave to exercise hot reload".to_owned();
    view.record("ended");
}

fn on_failed(failure: On<StoryFailed>, mut view: ResMut<GameViewModel>) {
    view.can_continue = false;
    view.choices.clear();
    view.status = "StoryFailed | inspect the author-facing diagnostic".to_owned();
    view.dialogue = failure.message.clone();
    view.failure = Some(failure.message.clone());
    view.record(format!("failed:{}", failure.message));
}

fn sync_observable_state(story: Res<WeaveStory>, mut view: ResMut<GameViewModel>) {
    let state = story.variables().map_or_else(
        || "Waiting for runtime state...".to_owned(),
        |variables| {
            if variables.is_empty() {
                "No declared variables".to_owned()
            } else {
                variables
                    .iter()
                    .map(|(name, state)| format!("{name}: {}", state.value))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        },
    );
    if view.observable_state != state {
        view.observable_state = state;
    }
}

fn setup_ui(mut commands: Commands) {
    commands.spawn(Camera2d);
    commands
        .spawn((
            Name::new("The Ember Forecast UI"),
            Node {
                width: vw(100),
                height: vh(100),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(28)),
                row_gap: px(18),
                ..default()
            },
            BackgroundColor(BACKGROUND),
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("THE EMBER FORECAST"),
                TextFont {
                    font_size: 34.0,
                    ..default()
                },
                TextColor(TEXT),
            ));
            root.spawn((
                Text::new("A tiny Weave + Bevy integration game"),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(MUTED),
            ));
            root.spawn((
                ViewText::Status,
                Text::new("Loading dialogue.weave through Bevy's asset pipeline..."),
                TextFont {
                    font_size: 15.0,
                    ..default()
                },
                TextColor(ACCENT),
            ));

            root.spawn(Node {
                width: percent(100),
                flex_grow: 1.0,
                flex_direction: FlexDirection::Row,
                column_gap: px(18),
                ..default()
            })
            .with_children(|main| {
                main.spawn((
                    Node {
                        width: percent(62),
                        height: percent(100),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(24)),
                        row_gap: px(18),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(18)),
                        ..default()
                    },
                    BackgroundColor(PANEL),
                    BorderColor::all(BORDER),
                ))
                .with_children(|dialogue_panel| {
                    dialogue_panel.spawn((
                        Text::new("DIALOGUE"),
                        TextFont {
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(ACCENT),
                    ));
                    dialogue_panel.spawn((
                        ViewText::Dialogue,
                        Text::new("The story will begin when its asset is ready."),
                        TextFont {
                            font_size: 28.0,
                            ..default()
                        },
                        TextColor(TEXT),
                        Node {
                            width: percent(100),
                            min_height: px(150),
                            ..default()
                        },
                    ));
                    dialogue_panel.spawn((
                        ChoiceList,
                        Node {
                            width: percent(100),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(10),
                            ..default()
                        },
                    ));
                    dialogue_panel
                        .spawn((
                            ContinueButton,
                            UiAction::Continue,
                            Button,
                            Node {
                                display: Display::None,
                                width: percent(100),
                                min_height: px(52),
                                padding: UiRect::axes(px(18), px(12)),
                                align_items: AlignItems::Center,
                                justify_content: JustifyContent::Center,
                                border: UiRect::all(px(1)),
                                border_radius: BorderRadius::all(px(10)),
                                ..default()
                            },
                            BackgroundColor(NORMAL_BUTTON),
                            BorderColor::all(BORDER),
                        ))
                        .with_children(|button| {
                            button.spawn((
                                Text::new("Continue  >"),
                                TextFont {
                                    font_size: 18.0,
                                    ..default()
                                },
                                TextColor(TEXT),
                            ));
                        });
                });

                main.spawn((
                    Node {
                        width: percent(38),
                        height: percent(100),
                        flex_direction: FlexDirection::Column,
                        padding: UiRect::all(px(20)),
                        row_gap: px(12),
                        border: UiRect::all(px(1)),
                        border_radius: BorderRadius::all(px(18)),
                        ..default()
                    },
                    BackgroundColor(PANEL_ALT),
                    BorderColor::all(BORDER),
                ))
                .with_children(|telemetry| {
                    telemetry.spawn((
                        Text::new("PATTERNDRAWN"),
                        TextFont {
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(ACCENT),
                    ));
                    telemetry.spawn((
                        ViewText::Pattern,
                        Text::new("Waiting for the omen spread..."),
                        TextFont {
                            font_size: 16.0,
                            ..default()
                        },
                        TextColor(TEXT),
                        Node {
                            width: percent(100),
                            min_height: px(90),
                            ..default()
                        },
                    ));
                    telemetry.spawn((
                        Text::new("OBSERVABLE STATE"),
                        TextFont {
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(ACCENT),
                    ));
                    telemetry.spawn((
                        ViewText::ObservableState,
                        Text::new("Waiting for runtime state..."),
                        TextFont {
                            font_size: 15.0,
                            ..default()
                        },
                        TextColor(TEXT),
                        Node {
                            width: percent(100),
                            min_height: px(100),
                            ..default()
                        },
                    ));
                    telemetry.spawn((
                        Text::new("STORY EVENTS"),
                        TextFont {
                            font_size: 13.0,
                            ..default()
                        },
                        TextColor(ACCENT),
                    ));
                    telemetry.spawn((
                        ViewText::EventLog,
                        Text::new("Waiting for StoryReady..."),
                        TextFont {
                            font_size: 12.0,
                            ..default()
                        },
                        TextColor(MUTED),
                        Node {
                            width: percent(100),
                            ..default()
                        },
                    ));
                });
            });

            root.spawn((
                Text::new(
                    "HOT RELOAD  |  Save examples/bevy-dialogue/assets/dialogue.weave while this window is open",
                ),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                TextColor(MUTED),
            ));
        });
}

#[allow(clippy::type_complexity)]
fn button_interactions(
    mut commands: Commands,
    mut input_focus: ResMut<InputFocus>,
    mut buttons: Query<
        (
            Entity,
            &Interaction,
            &UiAction,
            &mut BackgroundColor,
            &mut BorderColor,
            &mut Button,
        ),
        Changed<Interaction>,
    >,
) {
    for (entity, interaction, action, mut background, mut border, mut button) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                input_focus.set(entity);
                *background = BackgroundColor(PRESSED_BUTTON);
                *border = BorderColor::all(ACCENT);
                button.set_changed();
                match *action {
                    UiAction::Continue => commands.weave_continue(),
                    UiAction::Choose(index) => commands.weave_choose(index),
                }
            }
            Interaction::Hovered => {
                input_focus.set(entity);
                *background = BackgroundColor(HOVERED_BUTTON);
                *border = BorderColor::all(ACCENT);
                button.set_changed();
            }
            Interaction::None => {
                if input_focus.get() == Some(entity) {
                    input_focus.clear();
                }
                *background = BackgroundColor(NORMAL_BUTTON);
                *border = BorderColor::all(BORDER);
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn sync_ui(
    mut commands: Commands,
    view: Res<GameViewModel>,
    mut texts: Query<(&ViewText, &mut Text)>,
    mut continue_buttons: Query<&mut Node, With<ContinueButton>>,
    choice_lists: Query<Entity, With<ChoiceList>>,
    choice_buttons: Query<Entity, With<ChoiceButton>>,
) {
    if !view.is_changed() {
        return;
    }

    let first_event = view.events.len().saturating_sub(8);
    for (role, mut text) in &mut texts {
        **text = match role {
            ViewText::Status => view.status.clone(),
            ViewText::Dialogue => view.dialogue.clone(),
            ViewText::Pattern if view.patterns.is_empty() => {
                "Waiting for the omen spread...".to_owned()
            }
            ViewText::Pattern => view.patterns.join("\n"),
            ViewText::ObservableState => view.observable_state.clone(),
            ViewText::EventLog if view.events.is_empty() => "Waiting for StoryReady...".to_owned(),
            ViewText::EventLog => view.events[first_event..].join("\n"),
        };
    }
    for mut node in &mut continue_buttons {
        node.display = if view.can_continue {
            Display::Flex
        } else {
            Display::None
        };
    }

    for entity in &choice_buttons {
        commands.entity(entity).despawn();
    }
    for list in &choice_lists {
        commands.entity(list).with_children(|parent| {
            for (index, label) in view.choices.iter().enumerate() {
                parent
                    .spawn((
                        Name::new(format!("Dialogue choice {}", index + 1)),
                        ChoiceButton,
                        UiAction::Choose(index),
                        Button,
                        Node {
                            width: percent(100),
                            min_height: px(50),
                            padding: UiRect::axes(px(18), px(12)),
                            align_items: AlignItems::Center,
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(10)),
                            ..default()
                        },
                        BackgroundColor(NORMAL_BUTTON),
                        BorderColor::all(BORDER),
                    ))
                    .with_children(|button| {
                        button.spawn((
                            Text::new(format!("{}. {label}", index + 1)),
                            TextFont {
                                font_size: 17.0,
                                ..default()
                            },
                            TextColor(TEXT),
                        ));
                    });
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_asset_exercises_choices_patterns_and_observable_state() {
        let source = include_str!("../assets/dialogue.weave");
        assert!(source.contains("=== start ==="));
        assert!(source.contains("omens.spread.day.draw()"));
        assert!(source.contains("SET cups = cups + 1"));
    }

    #[test]
    fn smoke_mode_loads_and_observes_the_complete_game_flow() {
        let mut app = build_smoke_app();
        drive_smoke_test(&mut app).expect("the packaged Bevy game should complete its smoke flow");
    }

    #[test]
    fn integration_targets_the_documented_bevy_release() {
        assert_eq!(weave_bevy::BEVY_VERSION, "0.18");
    }
}
