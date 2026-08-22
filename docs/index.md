# Weave documentation

Weave is a narrative scripting language, deterministic Rust runtime, Bevy integration, and visual editor for branching stories with meaning-bearing pattern systems.

This site documents language version **0.1.0**. Start with the [five-minute quickstart](getting_started.md) to compile and run a complete story, then choose the path that matches your work:

- Authors: learn the [language](language_guide.md), [pattern systems](pattern_systems.md), [community packages](community_patterns.md), and [visual editor](editor_guide.md).
- Game developers: use the standalone [Rust APIs](api_reference.md), [Bevy plugin](bevy_integration.md), or [browser runtime](web_player.md).
- Tool builders: integrate the [language server](language_server.md) and [Tree-sitter grammar](tree_sitter.md).
- Contributors: read the [architecture](architecture.md), [contribution guide](contributing.md), and [security policy](security.md).

## What is stable in 0.1.0?

The source-language version, runtime IR version, JSON Schema, saved-state boundaries, and editor integrations each publish an explicit compatibility value. See [version and compatibility](versioning.md) before persisting or exchanging compiled stories.

All examples in this site are checked by the documentation build. Runnable `.weave` sources compile with the current workspace, headless Rust and Bevy examples execute to completion, generated browser JSON is compared with its source, and public Rust API documentation is regenerated.

## Get the source

```bash
git clone https://github.com/chrisgliddon/weave.git
cd weave
cargo test --workspace
```

The repository is MIT licensed. Never publish credentials in an issue, log, fixture, screenshot, or example; replace every credential value with `[REDACTED]`.
