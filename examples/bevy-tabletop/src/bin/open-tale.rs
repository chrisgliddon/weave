//! Bevy consumer for the Open Tale reference fixture.
//!
//! Loads the checked-in creation, initial state, ordered eleven-step playthrough, and final state,
//! replays every request with `weave-tabletop`'s trusted resolver, and requires each generated
//! receipt and the final state to match the recorded artifacts.
//!
//! Run with `cargo run -p weave-example-bevy-tabletop --bin open-tale`.

use bevy::prelude::*;
use weave_example_bevy_tabletop::{Adapter, fixture_root, replay_adapter, summary_line};

#[derive(Resource)]
struct OpenTaleOutcome(Result<weave_example_bevy_tabletop::AdapterRun, String>);

fn main() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .add_systems(Startup, load_and_replay);
    app.update();

    let outcome = app.world().resource::<OpenTaleOutcome>();
    match &outcome.0 {
        Ok(run) => {
            println!("Open Tale Bevy consumer");
            println!("{}", summary_line(run));
            for (index, step) in run.steps.iter().enumerate() {
                println!(
                    "{:02} {} | {} | receipt matches | events: {}",
                    index + 1,
                    step.id,
                    step.request.operation,
                    step.event_kinds.join(", ")
                );
            }
            println!("final revision {}", run.final_state.revision);
            println!("final entropy cursor {}", run.final_state.entropy.cursor);
            println!("observable values:");
            for line in &run.observable {
                println!("  {line}");
            }
            println!(
                "Replayed all {} recorded steps and matched the checked-in final state.",
                run.steps.len()
            );
        }
        Err(error) => {
            eprintln!("Open Tale Bevy consumer failed: {error}");
            std::process::exit(1);
        }
    }
}

fn load_and_replay(mut commands: Commands) {
    commands.insert_resource(OpenTaleOutcome(replay_adapter(
        Adapter::OpenTale,
        &fixture_root(),
    )));
}
