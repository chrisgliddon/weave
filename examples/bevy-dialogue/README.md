# The Ember Forecast

This small Bevy 0.18 game is the complete Weave engine-integration reference. It loads and compiles `assets/dialogue.weave` through Bevy's asset pipeline, renders story lines and choices with Bevy UI, displays observable runtime variables, and turns every `PatternDrawn` element into an omen entry.

Run the interactive game from the repository root:

```bash
cargo run -p weave-example-bevy-dialogue
```

Choose responses with the onscreen buttons. While the window is open, edit and save `examples/bevy-dialogue/assets/dialogue.weave`; the file watcher recompiles it and the status panel reacts to `StoryReloaded`. Compatible story state is preserved by `WeavePlugin` when the pattern definitions have not changed.

For CI, remote shells, and automated verification, run the same observers and story model without a window:

```bash
cargo run -p weave-example-bevy-dialogue -- --smoke-test
```

Smoke mode loads the real asset, auto-advances through one choice, and fails unless the game observes ready, ordered pattern draws, lines, choices, observable state mutation, end-of-story, and reload feedback without a `StoryFailed` event.

The example deliberately uses Bevy's built-in font and flat-color UI. There are no art assets, scene files, or example-only runtime APIs to obscure the integration boundary.
