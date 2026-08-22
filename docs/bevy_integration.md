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
        .add_observer(on_pattern)
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

fn on_pattern(draw: On<PatternDrawn>) {
    println!(
        "{} / {} / {:?} / {:?} / reversed={}",
        draw.system, draw.element, draw.position, draw.meaning, draw.reversed
    );
}

fn on_failed(failure: On<StoryFailed>) {
    eprintln!("{}", failure.message);
}
```

Queue a selection with `commands.weave_choose(index)`. `WeaveStory::variables` exposes the standalone runtime's deterministic variable map without introducing Bevy types into `weave-runtime`. Runtime failures retain their structured `RuntimeError` and source span in `StoryFailed::runtime_error`.

`PatternDrawn` is triggered once for each result element. Its payload owns the system and element identities, optional spread position, conventional name and effective meaning, reversal flag, and complete semantic field map. A single draw produces one event. A spread produces events in declared position order. All of those events run before the `DeliverLine`, `DeliverChoices`, or `StoryEnded` boundary reached by the same action, so observers see narrative evaluation order.

Hot reload restores story state when pattern IR is unchanged. If a pattern definition or draw configuration changes, the runtime is rebuilt from the configured seed instead; this prevents old element identities or semantic objects from leaking into the reloaded story. `StoryReloaded` is emitted after either valid path.

Launch the complete interactive example game with:

```sh
cargo run -p weave-example-bevy-dialogue
```

The window displays delivered dialogue, dynamic choices, observable variables, `PatternDrawn` results, story-event history, and `StoryReloaded` status. Edit `examples/bevy-dialogue/assets/dialogue.weave` while it runs to exercise Bevy's file watcher. The same integration has a deterministic window-free smoke mode:

```sh
cargo run -p weave-example-bevy-dialogue -- --smoke-test
```

## Portable domain-module data

Compiled module values are ordinary `StoryIr` data; a Bevy host does not need the editor or an artifact registry at runtime. The finite domain example deserializes the canonical tracer RON, reads its typed exports through `DomainModuleIr::value`, inserts them as a Bevy `Resource`, reports them from `Startup`, and exits after one update:

```sh
cargo run -p weave-example-domain-module-bevy
```

Use this boundary when a build pipeline resolves and compiles domain artifacts ahead of the game. The [domain-module guide](domain_modules.md) documents source activation and compatibility validation.
