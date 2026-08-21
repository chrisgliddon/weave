# Bevy integration

Weave targets Bevy 0.18. Add `WeavePlugin`, install one `WeaveStory` resource, and respond to observer events. Source `.weave` files are compiled by the asset loader; compiled `.ron` files load through the same asset type. With Bevy's `file_watcher` feature enabled, changed assets rebuild the runtime and emit `StoryReloaded`.

```rust,no_run
use bevy::prelude::*;
use weave_bevy::prelude::*;

fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins((DefaultPlugins, WeavePlugin))
        .insert_resource(WeaveStory::new("stories/dialogue.weave"))
        .add_observer(on_ready)
        .add_observer(on_line)
        .add_observer(on_choices)
        .add_observer(on_failed);
    app
}

fn on_ready(_: On<StoryReady>, mut commands: Commands) {
    commands.weave_jump_to("start");
}

fn on_line(line: On<DeliverLine>, mut commands: Commands) {
    println!("{}", line.text);
    commands.weave_continue();
}

fn on_choices(choices: On<DeliverChoices>) {
    for (index, choice) in choices.iter().enumerate() {
        println!("{index}: {}", choice.text);
    }
}

fn on_failed(failure: On<StoryFailed>) {
    eprintln!("{}", failure.message);
}
```

Queue a selection with `commands.weave_choose(index)`. `WeaveStory::variables` exposes the standalone runtime's deterministic variable map without introducing Bevy types into `weave-runtime`. Runtime failures retain their structured `RuntimeError` and source span in `StoryFailed::runtime_error`.

The complete finite, headless example runs with:

```sh
cargo run -p weave-example-bevy-dialogue
```
