# The Ember Forecast

This small Bevy 0.18 game is the complete Weave engine-integration reference. It loads and compiles `assets/dialogue.weave` through Bevy's asset pipeline, folds the runtime's presentation cues into a portable `PlayFrame`, and renders a staged scene with Bevy UI.

Run the interactive game from the repository root:

```bash
cargo run -p weave-example-bevy-dialogue
```

Choose responses with the onscreen buttons. While the window is open, edit and save `examples/bevy-dialogue/assets/dialogue.weave`; the file watcher recompiles it and the status panel reacts to `StoryReloaded`.

For CI, remote shells, and automated verification, run the same observers and scene projection without a window:

```bash
cargo run -p weave-example-bevy-dialogue -- --smoke-test
```

Smoke mode loads the real asset, auto-advances through the first choice, and fails unless the game observes ready, ordered pattern draws, lines, choices, observable state mutation, end-of-story, reload feedback, and the expected presentation cue order and frame.

## Portable scene contract

The scene is driven by the compiled story's declared presentation data and the runtime's `PresentationEvent` cues, in source order:

- `@scene` selects a background with its `fit` and `anchor`.
- `@show`, `@hide`, `@move`, and `@swap` stage characters with slot, layer, and variant.
- `@speaker` names the current speaker; a bare cue means narration.
- `@transition` records a bounded transition for the next staged operation.

`weave_bevy::play` implements the fold. It has no editor or GPUI dependency, so the same `PlayFrame` can be produced by another renderer from the same `StoryIr` and cue list. Frame order is deterministic: cues apply in source order, stages keep first-show order, and painting sorts by layer then insertion order.

A fixed seed and fixed choices reproduce the same cue log and the same final frame. The example uses seed `7` and the first choice in smoke mode; the `the_same_seed_and_choices_produce_the_same_cue_order_and_frame` test checks both runs.

Fallbacks are total:

- an undeclared scene or stage uses contract defaults;
- an unknown variant keeps the stage's current image and records a note;
- a missing image path never panics. This example does not load image files. Declared paths stay in the frame, and the scene draws deterministic colour blocks for every background and character.

## Synthetic fixture and artwork

`assets/dialogue.weave` is an original synthetic fixture written for this example. The `*_placeholder.png` paths are fictional references that exercise the missing-asset contract; no binary artwork is stored or loaded. All scene and stage visuals are generated at runtime from deterministic colour hashes, so there is no third-party art to license.

The example deliberately uses Bevy's built-in font and flat-colour UI. There are no scene files or example-only runtime APIs to obscure the integration boundary.
