# Weave

*A narrative scripting language that merges generative grammar with branching interactive fiction, powered by pattern-based meaning systems.*

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![Bevy](https://img.shields.io/badge/bevy-0.15%2B-blueviolet.svg)](https://bevyengine.org)
[![GPUI](https://img.shields.io/badge/editor-GPUI-green.svg)](https://gpui.rs)

---

## What is Weave?

Weave is a narrative scripting language and runtime that combines the best of two worlds:

- **[Tracery](https://docs.rs/tracery)** — Kate Compton's generative grammar language for procedural text expansion
- **[Ink](https://github.com/inkle/ink)** — Inkle's branching narrative scripting language for interactive fiction

…and adds something neither has: **pattern systems** — meaning-bearing procedural generators like tarot cards, I-Ching hexagrams, and runes that produce structured, semantically rich results your narrative can reason about.

Weave compiles to RON or JSON, runs as a Bevy plugin, and ships with a standalone GPU-accelerated node-based editor built in GPUI.

---

## Why Weave?

| Feature | Tracery | Ink | **Weave** |
|---------|---------|-----|-----------|
| Generative grammar (`#key#` inline expansion) | ✅ | ❌ | ✅ |
| Branching narrative (choices, diverts, weaves) | ❌ | ✅ | ✅ |
| Variables & logic | ❌ | ✅ | ✅ |
| State tracking (lists, flags, state machines) | ❌ | ✅ | ✅ |
| Pattern systems (tarot, I-Ching, runes, custom) | ❌ | ❌ | ✅ |
| Compile to RON / JSON | JSON only | JSON only | ✅ |
| Visual node-based editor | ❌ | Inky (Electron) | ✅ (GPU-accelerated) |
| Bevy integration | ❌ | `bevy_bladeink` | ✅ (first-class) |
| Hot reload | ❌ | ❌ | ✅ |

---

## A Taste of Weave

```weave
// --- Grammar: scoped generative rules (from Tracery) ---
grammar names {
    first: ["Aldric", "Mira", "Theron", "Lyssia"],
    last:  ["the Bold", "Whisperwind", "Ironhand"],
    full:  "#first# #last#"
}

grammar atmosphere {
    mood:     ["sombre", "electric", "anticipatory", "hushed"],
    weather:  ["rain sheeting down", "a clearing sky", "oppressive heat"],
    scene:    "The tavern was #mood#, with #weather# outside."
}

// --- Pattern: meaning-bearing procedural generation (NEW) ---
pattern tarot {
    major: [
        (name: "The Fool",  meaning: new_beginnings,  element: air),
        (name: "The Tower", meaning: sudden_change,  element: fire),
        (name: "The Star",  meaning: hope,            element: water),
        // ...78 cards
    ]
    spread three_card { positions: [past, present, future] }
}

// --- Narrative: branching structure (from Ink) ---
=== arrival ===
VAR current_card = tarot.spread.three_card.draw()
VAR npc = #names.full#

#atmosphere.scene#
A fortune teller waves you over. "#npc#, was it? The cards have been calling."

She lays out three cards.

* [Look at the cards]
    -> reading

* [Ask the price first]
    "Five silver." She doesn't look up.
    * [Pay]   -> reading
    * [Leave] -> leaving

=== reading ===
"Your past," she says, turning {current_card.past.name}.
{current_card.past.meaning == new_beginnings:
    "A journey begun in innocence," she murmurs.
- current_card.past.meaning == sudden_change:
    "Chaos shaped you, whether you know it or not."
- else:
    She studies the card in silence.
}

"Your future..." She turns the last card.
{current_card.future.name == "The Tower":
    Her face goes pale. "Oh, dear."
- current_card.future.name == "The Star":
    A smile breaks across her face. "There is hope for you yet."
- else:
    "The cards are... inconclusive."
}
-> after_reading

=== after_reading ===
You leave with the weight of {current_card.future.name} on your mind.
-> END
```

## Pattern Systems

Pattern systems are Weave's signature feature. They generate not just random output, but **structured results with semantic meaning** that your narrative can branch on, display, and remember.

### Built-in Pattern Systems

| System | Contents | Draw Methods |
|--------|----------|-------------|
| **Tarot** | 78 cards (22 major + 56 minor arcana) | Uniform, weighted, spread-based |
| **I-Ching** | 64 hexagrams | Coin method, yarrow stalks |
| **Runes** | 24 Elder Futhark runes | Single draw, three-rune spread |
| **Custom** | Anything you define | Anything you implement |

### Defining a Custom Pattern System

```weave
pattern weather_omens {
    omens: [
        (name: "Storm Crow",   meaning: ill_tidings,   severity: 3),
        (name: "Sun Dog",      meaning: good_fortune,  severity: 1),
        (name: "Frost Wolf",   meaning: harsh_winter,  severity: 4),
    ]
    draw: weighted_by_severity
    spread day_omen { positions: [dawn, noon, dusk] }
}
```

Your narrative can then test against the **meaning**, not just the name:

```weave
{weather_omens.day_omen.dawn.meaning == ill_tidings:
    A crow circles the village three times. No one speaks.
- else:
    The dawn breaks clean and clear.
}
```

---

## Compilation

Weave source files (`.weave`) compile to an intermediate representation for runtime consumption.

```
story.weave  →  [weavec compiler]  →  story.ron   (Rust-native, human-readable)
                                   ↘  story.json  (interop, web, save/load)
```

### RON Output (excerpt)

```ron
Story(
    grammars: {
        "names": Grammar(rules: {
            "first": ["Aldric", "Mira", "Theron", "Lyssia"],
            "full":  "#first# #last#"
        })
    },
    patterns: {
        "tarot": PatternSystem(
            elements: [PatternElement(name: "The Fool", meaning: NewBeginnings)],
            draw_method: UniformRandom
        )
    },
    knots: {
        "arrival": Knot(content: [
            GrammarExpand("atmosphere.scene"),
            Choice(once: true, text: "Look at the cards", divert: "reading")
        ])
    }
)
```

---

## Repository Structure

```
weave/
├── crates/
│   ├── weave-core/          # Language parser, AST, type system
│   ├── weave-runtime/       # Story runtime engine (no Bevy dependency)
│   ├── weave-patterns/      # Built-in pattern systems (tarot, i-ching, runes)
│   ├── weave-compiler/      # .weave → .ron / .json compiler
│   ├── weave-bevy/          # Bevy plugin
│   └── weave-fmt/           # Formatter / pretty-printer for .weave files
│
├── editor/                  # Standalone GPU-accelerated editor (GPUI)
│   ├── src/
│   │   ├── main.rs
│   │   ├── editor/
│   │   │   ├── canvas.rs        # Node graph canvas (gpui-flow)
│   │   │   ├── node_types/      # Custom renderers per Weave construct
│   │   │   ├── panels/          # Code view, play view, inspector, pattern browser
│   │   │   └── theme.rs
│   │   ├── compiler/           # .weave ↔ graph model ↔ .ron/.json
│   │   └── project/            # File watching, project management
│   └── Cargo.toml
│
├── examples/                # Example stories and Bevy integration demos
│   ├── fortune_teller/      # The example shown above
│   ├── i_ching_divination/ # Pattern system showcase
│   └── bevy_dialogue/      # Bevy game integration
│
├── docs/
│   ├── language_guide.md   # Full syntax documentation
│   ├── pattern_systems.md  # How to define and use pattern systems
│   ├── bevy_integration.md # Using Weave in Bevy games
│   └── editor_guide.md     # Using the visual editor
│
├── Cargo.toml              # Workspace root
└── README.md
```

---

## Using Weave in Bevy

```rust
use bevy::prelude::*;
use weave_bevy::prelude::*;

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(WeavePlugin)
        .insert_resource(WeaveStory::new("stories/fortune_teller.weave"))
        .add_observer(on_story_ready)
        .add_observer(on_line_delivered)
        .add_observer(on_choices_delivered)
        .add_observer(on_pattern_drawn)  // React to tarot draws, etc.
        .run();
}

fn on_story_ready(_: On<StoryReady>, mut commands: Commands) {
    commands.weave_jump_to("arrival");
}

fn on_line_delivered(line: On<DeliverLine>, mut ui: ResMut<DialogueUI>) {
    ui.show_text(&line.text);
    commands.weave_continue();
}

fn on_choices_delivered(choices: On<DeliverChoices>, mut ui: ResMut<DialogueUI>) {
    for (i, choice) in choices.iter().enumerate() {
        ui.show_choice(i, choice.text());
    }
}

fn on_pattern_drawn(draw: On<PatternDrawn>, mut ui: ResMut<CardDisplay>) {
    // Show the drawn tarot card in your game UI
    ui.reveal_card(&draw.name, &draw.position, draw.is_reversed);
}
```

### Using Weave Without Bevy (Standalone Runtime)

```rust
use weave_runtime::Story;

let mut story = Story::from_file("stories/fortune_teller.ron")?;

// Step through the narrative
while story.can_continue() {
    let line = story.continue();
    println!("{line}");
}

if story.has_choices() {
    for (i, choice) in story.choices().iter().enumerate() {
        println!("  {i}: {}", choice.text);
    }
    story.choose(0);
}
```

---

## The Editor

Weave ships with a standalone visual editor built in [GPUI](https://gpui.rs) (Zed's GPU-accelerated UI framework) and [gpui-flow](https://github.com/pacifio/gpui-flow) for the node graph canvas.

### Dual-View Design

| View | Description |
|------|-------------|
| **Node Graph** | Twine-like canvas. Knots are nodes, diverts are edges, choices branch. Grammar and pattern systems have specialized node types. Pan, zoom, drag, multi-select. |
| **Text Editor** | Raw `.weave` source with syntax highlighting, inline errors, and live preview. Two-way synced with the graph. |
| **Play Preview** | Live story runner. See generated output with current pattern draws. Replay from any knot. |
| **Pattern Browser** | Visual deck/hexagram browser. See available pattern systems, inspect draw methods, preview spreads. |

### Node Types in the Graph

| Node Type | Visual | Handles |
|-----------|--------|---------|
| **Knot** | Card with name + text preview | Diverts in (left), diverts out (right) |
| **Choice** | Diamond with condition label | Branching handles for each option |
| **Grammar** | Rounded box with key + sample expansion | Reference handles to usage sites |
| **Pattern** | Hexagon with system icon + sample draw | Output handle carrying structured result |
| **Variable** | Small pill with name + type | Read/write handles |
| **Divert** | Edge between knots | — |
| **Thread** | Dotted edge | — |

### Running the Editor

```bash
cargo run -p weave_editor
```

---

## Installation

### As a Bevy Dependency

```toml
[dependencies]
weave-bevy = "0.1"
```

### As a Standalone Runtime

```toml
[dependencies]
weave-runtime = "0.1"
```

### The Compiler CLI

```bash
# Install
cargo install weavec

# Compile a story
weavec story.weave                    # → story.ron
weavec story.weave --format json      # → story.json
weavec story.weave --watch            # recompile on file change
```

---

## Roadmap

### Phase 1 — Core Language & Runtime
- [ ] Language specification document
- [ ] Parser (PEG grammar via `pest`)
- [ ] AST + type checker
- [ ] Runtime: grammar expansion, flow control, variables, lists
- [ ] Compile to RON
- [ ] Bevy plugin (events, resources, hot reload)
- [ ] Basic example stories

### Phase 2 — Pattern Systems
- [ ] `PatternSystem` trait design
- [ ] Tarot (78 cards, spreads, reversals)
- [ ] I-Ching (64 hexagrams, coin + yarrow methods)
- [ ] Runes (Elder Futhark)
- [ ] Custom pattern system authoring
- [ ] `PatternDrawn` events in Bevy

### Phase 3 — Editor
- [ ] GPUI application skeleton
- [ ] Node graph canvas (gpui-flow integration)
- [ ] Custom node renderers for each construct
- [ ] Text editor view with syntax highlighting
- [ ] Bidirectional sync (graph ↔ text)
- [ ] Live play preview
- [ ] Pattern browser panel
- [ ] Project management + file watching

### Phase 4 — Ecosystem
- [ ] JSON compilation output
- [ ] Web-based story player (WASM)
- [ ] Language server (LSP) for text editors
- [ ] Syntax highlighting grammars (tree-sitter)
- [ ] Documentation site
- [ ] Example game integration
- [ ] Community pattern system library

---

## Design Philosophy

1. **Text first.** Writers should be able to work in plain text. The visual editor is a tool, not a prison. Every feature available in the graph is available in text, and vice versa.

2. **Meaning, not just randomness.** Procedural generation should produce results with semantic weight. A tarot draw isn't just "you got card #17" — it's "you got The Star, which means hope, and your narrative should know that."

3. **Compile, don't interpret.** Source files compile to a compact intermediate representation. The runtime loads compiled stories, not source. This keeps runtime fast and embeddable.

4. **Bevy is a first-class citizen, not an afterthought.** Story state lives in the ECS. Variables are observable resources. Pattern draws are events. Hot reload uses the asset pipeline.

5. **The editor is GPU-accelerated.** Large node graphs (thousands of knots) should feel smooth. No Electron, no browser canvas, no CPU-bound immediate-mode fallbacks.

6. **Composability over monolith.** Grammar blocks, pattern systems, and narrative knots are independently authored and composed. You can ship a grammar pack, a pattern system, or a story module separately.

---

## Acknowledgments

Weave stands on the shoulders of:

- **[Kate Compton](https://galaxykate.com/)** — creator of [Tracery](https://github.com/galaxykate/tracery), the generative grammar language that proved procedural text could be simple and beautiful.
- **[Inkle](https://www.inklestudios.com/)** — creators of [Ink](https://github.com/inkle/ink), the narrative scripting language that proved branching fiction could be writer-friendly and powerful.
- **[Zed Industries](https://zed.dev/)** — creators of [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui), the GPU-accelerated UI framework that proved Rust can do fast, smooth desktop UI.
- **[pacifio](https://github.com/pacifio/gpui-flow)** — creator of gpui-flow, the node graph editor for GPUI.
- **[Bevy](https://bevyengine.org/)** — the game engine that made data-driven Rust game development a reality.

---

## License

MIT — see [LICENSE](LICENSE).

## Contributing

Contributions welcome. The project is in early development — see the roadmap for what's needed most.

Open an issue first for large changes. Keep PRs focused.

---

<p align="center">
  <em>Every story is a pattern. Every pattern has meaning. Weave them together.</em>
</p>
