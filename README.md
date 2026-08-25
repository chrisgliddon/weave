# Weave

*A narrative scripting language that merges generative grammar with branching interactive fiction, powered by pattern-based meaning systems.*

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.93%2B-orange.svg)](https://www.rust-lang.org)
[![Bevy](https://img.shields.io/badge/bevy-0.18-blueviolet.svg)](https://bevyengine.org)
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
    builtin: tarot
    draw: uniform
    reversals: true
    duplicates: false
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
"Your {current_card.past.position}," she says, turning {current_card.past.name}.
{current_card.past.reversed:
    "The card is reversed," she adds.
- else:
    "The card is upright."
}
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
| **Custom** | Authored semantic records | Uniform, weighted, spread-based |
| **Community package** | Versioned, licensed, provenance-tracked semantic data | Uniform, weighted, spread-based |

Built-ins are declared explicitly and may be given any source-level name:

```weave
pattern cards {
    builtin: tarot
    reversals: true
}

pattern changes {
    builtin: i_ching
    draw: yarrow_stalks
}

pattern runes {
    builtin: elder_futhark
}
```

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

Your narrative can then draw once and test against the **meaning**, not just the name:

```weave
VAR omen = weather_omens.spread.day_omen.draw()

{omen.dawn.meaning == ill_tidings:
    A crow circles the village three times. No one speaks.
- else:
    The dawn breaks clean and clear.
}
```

Reusable third-party systems use the strict, data-only [community package format](docs/community_patterns.md). The `weave-pattern` CLI validates, stages, installs, and indexes packages; `weavec --pattern-registry ... --pattern 'id@version'` embeds an explicitly selected package into ordinary story IR without executing package code or contacting a remote registry.

Pluggable world, character, and ruleset data share the declarative [domain-module contract](docs/domain_modules.md). The host-independent `weave-domain` crate defines closed manifests, typed packs and maps, protected read-only paths, stable entity metadata, immutable registries, bounded project discovery, exact locks, deterministic dependency ordering, and machine-readable provenance. A text-first `module` declaration activates a compatible installed release and may add permitted typed fictional `override` values without mutating pack provenance; the compiler and editor discover only approved project-relative artifacts, type-check their paths, and embed effective values plus authored lineage in IR 4. Build one with the [third-party tutorial](docs/domain_module_tutorial.md), follow the four-preset [Weave World guide](docs/world_module.md), follow the [Weave Character guide](docs/character_module.md) from a provenance-aware six-factor/24-facet profile through guided questionnaires, explicit conflict review, reviewed enrichments, read-only corpus health, and portable hosts, or inspect the [selectable tabletop adapter contract](docs/tabletop_adapters.md) for exact capability discovery, isolated state, deterministic replay, event visibility, switching, the MIT/CC0/Apache public-source gate, and verified end-to-end Plug-And-Play, Dungeonpunk, and Freehack implementations. Portable fixtures remain data-only and do not load third-party code or depend on the editor.

---

## Compilation

Weave source files (`.weave`) compile to an intermediate representation for runtime consumption.

```
story.weave  →  [weavec compiler]  →  story.ron   (Rust-native, human-readable)
                                   ↘  story.json  (interop, web, save/load)
```

RON and JSON decode to the same versioned `StoryIr`. The compatibility rules and generated JSON Schema are documented in the [JSON story format guide](docs/json_format.md).

### Browser player

The dependency-light [`weave-web`](crates/weave-web) crate exposes the standalone runtime through WebAssembly. Build and serve the accessible example with:

```bash
cargo install wasm-bindgen-cli --version 0.2.127 --locked --root target/web-tools
./scripts/build-web-player.sh
python3 -m http.server 4173 --directory examples/web-player
```

See the [web player guide](examples/web-player/README.md) for its JavaScript API, deterministic seed contract, state-storage boundary, platform requirements, and measured release size.

### Language server

The editor-independent `weave-lsp` binary provides compiler diagnostics, workspace navigation, completion, rename, hover types, symbols, and canonical formatting over standard LSP stdio:

```bash
cargo install --path crates/weave-lsp
weave-lsp
```

Editors normally launch the process themselves. See the [language server guide](docs/language_server.md) for capabilities, workspace behavior, and a Neovim configuration.

### Syntax highlighting

[`tree-sitter-weave`](tree-sitter-weave) provides an incremental parser plus highlight, local-variable, and symbol-tag queries for `.weave` files. It recognizes the complete language surface, including domain-module activations, nested narrative blocks, grammar references, pattern declarations, and incomplete editing states:

```bash
cd tree-sitter-weave
npm ci
npm run test:grammar
npm test
```

Editor and host integration instructions, generated-parser policy, and the language-version contract are documented in the [grammar package guide](tree-sitter-weave/README.md).

### Documentation site

The versioned, searchable documentation site is built from the canonical guides in this repository and publishes generated Rust API documentation alongside them. Build the same artifact locally with:

```bash
cargo install mdbook --version 0.5.4 --locked
python3 scripts/build-docs.py
```

The build compiles every ordinary example story, verifies domain, World composition/export, and Character corpus-health schemas and goldens, packaging, transitive locks, the third-party tutorial, tracer and composed World RON/JSON outputs, runs the finite Rust examples, tests and bundles the PixiJS consumer, checks browser JSON, regenerates rustdoc, and rejects broken local links or missing accessibility structure. This local command is the canonical documentation gate; the repository intentionally has no GitHub Actions or GitHub Pages deployment workflow.

### RON Output (excerpt)

```ron
StoryIr(
    version: 4,
    grammars: {
        // Lowered templates omitted.
    },
    patterns: {
        "tarot": PatternSystemIr(
            builtin: Some(tarot),
            collections: {},
            spreads: {
                "three_card": SpreadIr(positions: ["past", "present", "future"]),
            },
            draw_method: PatternDrawMethodIr(kind: "uniform"),
            allow_duplicates: false,
            reversals: true,
        ),
    },
    modules: {
        // Exact selected module and pack versions plus validated exports.
    },
    // Lowered knots omitted.
)
```

---

## Repository Structure

```
weave/
├── crates/
│   ├── weave-core/          # Language parser, AST, type system
│   ├── weave-runtime/       # Story runtime engine (no Bevy dependency)
│   ├── weave-tabletop/      # Adapter manifests, state, resolver, events, and license gate
│   ├── weave-patterns/      # Built-ins plus data-only community packages and registry CLI
│   ├── weave-domain/        # Versioned domain-module manifests, packs, validation, and schemas
│   ├── weave-character/     # Portable character profiles, extensions, and deterministic synthesis
│   ├── weave-world/         # Deterministic World composition, exports, and place lineage
│   ├── weave-world-corpus/  # Offline environmental source records → canonical World packs
│   ├── weave-compiler/      # .weave → .ron / .json compiler
│   ├── weave-bevy/          # Bevy plugin
│   ├── weave-web/           # Browser-safe WASM runtime bindings
│   ├── weave-fmt/           # Formatter / pretty-printer for .weave files
│   └── weave-lsp/           # Editor-independent language server
│
├── tree-sitter-weave/       # Incremental parser and editor syntax queries
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
├── examples/                # Runnable language, pattern, and integration stories
│   ├── stories/             # Basic grammar and branching source files
│   ├── standalone-runtime/  # Compiler + runtime example
│   ├── bevy-dialogue/       # Interactive Bevy dialogue game + smoke test
│   ├── domain-modules/      # Contract fixtures and compiled synthetic tracer
│   ├── domain-module-bevy/  # Portable tracer/composed World RON consumed as Bevy resources
│   ├── domain-module-pixijs/ # Portable tracer/composed World JSON rendered with PixiJS
│   ├── tabletop-adapters/   # Synthetic contract plus verified Plug-And-Play/Dungeonpunk/Freehack fixtures
│   └── web-player/          # Accessible no-bundler WASM player
│
├── patterns/                # Reviewed community packages and original examples
│
├── docs/
│   ├── index.md            # Documentation site landing page
│   ├── language_guide.md   # Full syntax documentation
│   ├── language_server.md  # LSP capabilities and editor setup
│   ├── pattern_systems.md  # How to define and use pattern systems
│   ├── community_patterns.md # Package format, registry, provenance, and moderation
│   ├── domain_modules.md    # Manifest, namespace, compatibility, and provenance contract
│   ├── tabletop_adapters.md # Adapter capabilities, state, replay, visibility, and licensing
│   ├── bevy_integration.md # Using Weave in Bevy games
│   ├── editor_guide.md     # Using the visual editor
│   └── api_reference.md    # Generated rustdoc entry points
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
        .add_observer(on_pattern_drawn)
        .add_observer(on_story_failed)
        .run();
}

fn on_story_ready(_: On<StoryReady>, mut commands: Commands) {
    commands.weave_jump_to("arrival");
}

fn on_line_delivered(
    line: On<DeliverLine>,
    mut ui: ResMut<DialogueUI>,
    mut commands: Commands,
) {
    ui.show_text(&line.text);
    commands.weave_continue();
}

fn on_choices_delivered(choices: On<DeliverChoices>, mut ui: ResMut<DialogueUI>) {
    for (i, choice) in choices.iter().enumerate() {
        ui.show_choice(i, &choice.text);
    }
}

fn on_pattern_drawn(draw: On<PatternDrawn>) {
    println!(
        "{} drew {} at {:?}: {:?}",
        draw.system, draw.element, draw.position, draw.meaning
    );
}

fn on_story_failed(failure: On<StoryFailed>) {
    eprintln!("{}", failure.message);
}
```

`PatternDrawn` owns its semantic payload and is emitted once per element, in spread position order, before the line or choice boundary that used the draw. See [the Bevy integration guide](docs/bevy_integration.md), launch the example game with `cargo run -p weave-example-bevy-dialogue`, or run its window-free gate with `cargo run -p weave-example-bevy-dialogue -- --smoke-test`.

### Using Weave Without Bevy (Standalone Runtime)

```rust
use weave_runtime::{Story, StoryEvent};

let mut story = Story::from_file("story.ron")?;

loop {
    match story.advance()? {
        StoryEvent::Line(line) => println!("{line}"),
        StoryEvent::Choices(choices) => {
            for (i, choice) in choices.iter().enumerate() {
                println!("  {i}: {}", choice.text);
            }
            story.choose(0)?;
        }
        StoryEvent::Ended => break,
    }
}
```

Run the complete standalone example with `cargo run -p weave-example-standalone`.

---

## The Editor

Roadmap Phase 3 will ship a standalone visual editor built in [GPUI](https://gpui.rs) (Zed's GPU-accelerated UI framework) and [gpui-flow](https://github.com/pacifio/gpui-flow) for the node graph canvas.

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
# Install from this checkout
cargo install --path crates/weave-compiler

# Compile a story
weavec story.weave                    # → story.ron
weavec story.weave --format json      # → story.json
weavec story.weave --watch            # recompile on file change
```

---

## Roadmap

### Phase 1 — Core Language & Runtime
- [x] Language specification document
- [x] Parser (PEG grammar via `pest`)
- [x] AST + type checker
- [x] Runtime: grammar expansion, flow control, variables, lists
- [x] Compile to RON
- [x] Bevy plugin (events, resources, hot reload)
- [x] Basic example stories

### Phase 2 — Pattern Systems
- [x] `PatternSystem` trait design
- [x] Tarot (78 cards, spreads, reversals)
- [x] I-Ching (64 hexagrams, coin + yarrow methods)
- [x] Runes (Elder Futhark)
- [x] Custom pattern system authoring
- [x] `PatternDrawn` events in Bevy

### Phase 3 — Editor
- [x] GPUI application skeleton
- [x] Node graph canvas (gpui-flow integration)
- [x] Custom node renderers for each construct
- [x] Text editor view with syntax highlighting
- [x] Bidirectional sync (graph ↔ text)
- [x] Live play preview
- [x] Pattern browser panel
- [x] Project management + file watching

### Phase 4 — Ecosystem
- [x] JSON compilation output
- [x] Web-based story player (WASM)
- [x] Language server (LSP) for text editors
- [x] Syntax highlighting grammars (tree-sitter)
- [x] Documentation site
- [x] Example game integration
- [x] Community pattern system library

### Phase 5 — Pluggable Domain Modules
- [x] Shared module manifests, namespaces, compatibility, and provenance contract
- [x] Weave World: reference-place shorthand, climate and environment data, named places, and layered rules
- [ ] Weave Character: personality, date context, relationships, expression, and guided authoring
- [x] Selectable tabletop ruleset adapters with isolated, versioned state
- [x] Portable RON and JSON domain packs with Bevy and PixiJS examples

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

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for development checks and the exact templates for Features, Improvements, and Bugs. The roadmap is tracked in `dex` and synchronized with GitHub Issues.

Open an issue before starting a large change and keep pull requests focused. Report vulnerabilities privately by following [SECURITY.md](SECURITY.md).

---

<p align="center">
  <em>Every story is a pattern. Every pattern has meaning. Weave them together.</em>
</p>
