use std::error::Error;
use std::thread;
use std::time::Duration;

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use weave_bevy::prelude::*;

#[derive(Resource, Default)]
struct ExampleStatus {
    finished: bool,
    failure: Option<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: format!("{}/assets", env!("CARGO_MANIFEST_DIR")),
            ..default()
        },
        WeavePlugin,
    ))
    .init_resource::<ExampleStatus>()
    .insert_resource(WeaveStory::new("dialogue.weave").with_seed(7))
    .add_observer(on_ready)
    .add_observer(on_line)
    .add_observer(on_choices)
    .add_observer(on_pattern)
    .add_observer(on_ended)
    .add_observer(on_failed);

    for _ in 0..2_000 {
        app.update();
        let status = app.world().resource::<ExampleStatus>();
        if let Some(failure) = &status.failure {
            return Err(failure.clone().into());
        }
        if status.finished {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(1));
    }
    Err("example timed out while loading its story asset".into())
}

fn on_ready(_: On<StoryReady>, mut commands: Commands) {
    commands.weave_continue();
}

fn on_line(line: On<DeliverLine>, mut commands: Commands) {
    println!("{}", line.text);
    commands.weave_continue();
}

fn on_choices(choices: On<DeliverChoices>, mut commands: Commands) {
    for (index, choice) in choices.iter().enumerate() {
        println!("  {index}: {}", choice.text);
    }
    commands.weave_choose(0);
}

fn on_pattern(draw: On<PatternDrawn>) {
    println!(
        "pattern {} / {} / {:?}: {:?}",
        draw.system, draw.element, draw.position, draw.meaning
    );
}

fn on_ended(_: On<StoryEnded>, mut status: ResMut<ExampleStatus>) {
    status.finished = true;
}

fn on_failed(failure: On<StoryFailed>, mut status: ResMut<ExampleStatus>) {
    status.failure = Some(failure.message.clone());
}

#[cfg(test)]
mod tests {
    #[test]
    fn example_asset_is_packaged() {
        let source = include_str!("../assets/dialogue.weave");
        assert!(source.contains("=== start ==="));
    }
}
