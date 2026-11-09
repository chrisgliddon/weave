//! Headless Bevy consumer for every checked-in tabletop adapter fixture.
//!
//! Loads and replays each adapter in a Bevy `Startup` system, then prints the observable final
//! values and the recorded event sequence. The process exits non-zero when a checked-in fixture
//! stops reproducing.
//!
//! Run every adapter, or pass one adapter key:
//!
//! ```text
//! cargo run -p weave-example-bevy-tabletop
//! cargo run -p weave-example-bevy-tabletop -- plug-and-play
//! ```

use bevy::prelude::*;
use weave_example_bevy_tabletop::{
    Adapter, AdapterRun, fixture_root, replay_adapter, replay_all, summary_line,
};

#[derive(Resource)]
struct RequestedAdapter(Option<Adapter>);

#[derive(Resource)]
struct ReplayOutcome(Result<Vec<AdapterRun>, String>);

fn main() {
    let requested = match std::env::args().nth(1) {
        None => None,
        Some(key) => match Adapter::ALL
            .into_iter()
            .find(|adapter| adapter.key() == key)
        {
            Some(adapter) => Some(adapter),
            None => {
                let keys = Adapter::ALL
                    .iter()
                    .map(|adapter| adapter.key())
                    .collect::<Vec<_>>()
                    .join(", ");
                eprintln!("unknown adapter `{key}`; expected one of: {keys}");
                std::process::exit(2);
            }
        },
    };

    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(RequestedAdapter(requested))
        .add_systems(Startup, load_and_replay);
    app.update();

    let outcome = app.world().resource::<ReplayOutcome>();
    match &outcome.0 {
        Ok(runs) => {
            println!("Bevy tabletop fixture replay");
            for run in runs {
                println!("\n{}", summary_line(run));
                println!("  final state matches the checked-in expectation");
                println!("  events:");
                for step in &run.steps {
                    println!("    {}: {}", step.id, step.event_kinds.join(", "));
                }
                println!("  observable values:");
                for line in &run.observable {
                    println!("    {line}");
                }
            }
            println!(
                "\nAll {} checked-in adapters replayed and matched their fixtures.",
                runs.len()
            );
        }
        Err(error) => {
            eprintln!("Bevy tabletop fixture replay failed: {error}");
            std::process::exit(1);
        }
    }
}

fn load_and_replay(requested: Res<RequestedAdapter>, mut commands: Commands) {
    let root = fixture_root();
    let outcome = match requested.0 {
        Some(adapter) => replay_adapter(adapter, &root).map(|run| vec![run]),
        None => replay_all(&root),
    };
    commands.insert_resource(ReplayOutcome(outcome));
}
