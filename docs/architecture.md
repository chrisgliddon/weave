# Weave Architecture

This document defines package ownership and dependency boundaries for the Rust workspace. The boundaries are part of the public architecture: new dependencies must not create cycles or move source-language concerns into integration crates.

## Packages

| Package | Responsibility | May depend on |
|---|---|---|
| `weave-core` | Source AST, spans, diagnostics, parser, static analysis, and versioned runtime IR | General-purpose parsing and serialization crates only |
| `weave-runtime` | Deterministic execution of compiled stories and saveable story/pattern state | `weave-core`, `weave-patterns`; never Bevy |
| `weave-patterns` | Serializable pattern extension boundary plus built-in tarot, I-Ching, and Elder Futhark data/algorithms | `weave-core` |
| `weave-compiler` | Checked AST-to-IR lowering, RON/JSON serialization, and the `weavec` CLI | `weave-core`; never `weave-runtime` or Bevy |
| `weave-fmt` | Canonical `.weave` source rendering | `weave-core` |
| `weave-bevy` | Bevy asset loading, hot reload, ECS resources, commands, and observer events | `weave-core`, `weave-compiler`, and `weave-runtime` |
| `weave_editor` | Standalone GPUI editor delivered in Phase 3 | Public APIs of the crates above |

The dependency direction is:

```text
weave-core
├── weave-patterns
├── weave-compiler
└── weave-fmt

weave-core + weave-patterns
└── weave-runtime

weave-core + weave-patterns + weave-compiler + weave-runtime
└── weave-bevy
```

`weave-core::ast` and `weave-core::ir` are deliberately separate models. The AST preserves author-facing syntax and source spans. The IR is a versioned, deterministic, runtime-facing schema and must not contain parser implementation types.

## Compatibility

- Workspace MSRV: Rust 1.93. The editor's pinned GPUI revision establishes this minimum; Bevy 0.18.1 remains supported.
- Serialized IR version: `weave_core::ir::IR_VERSION`.
- Bevy integration target: Bevy 0.18.
- Phase 2 source files use the language contract in [`language_guide.md`](language_guide.md).

Changing source semantics requires updating the language guide and parser fixtures. Changing serialized IR requires an explicit version decision and compatibility tests; the [JSON format contract](json_format.md) and its checked-in schema are authoritative for interoperable hosts. `PatternDefinition` and `StoryIr` are immutable/shareable. `PatternState` is caller-owned, serialized inside `StoryState`, and reconciled against the current definition on restore. The runtime owns only trait objects and never switches on Tarot, I-Ching, or rune identities; specialization lives behind `weave-patterns::PatternSystem`.

## Quality gates

The workspace gate is:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

Focused crates add golden, property, and integration tests where their acceptance criteria require them.
