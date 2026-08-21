# Weave Architecture

This document defines package ownership and dependency boundaries for the Rust workspace. The boundaries are part of the public architecture: new dependencies must not create cycles or move source-language concerns into integration crates.

## Packages

| Package | Responsibility | May depend on |
|---|---|---|
| `weave-core` | Source AST, spans, diagnostics, parser, static analysis, and versioned runtime IR | General-purpose parsing and serialization crates only |
| `weave-runtime` | Deterministic execution of compiled stories and saveable story state | `weave-core`; never Bevy |
| `weave-patterns` | Pattern extension boundary and, in Phase 2, built-in tarot, I-Ching, and rune data | `weave-core` |
| `weave-compiler` | Checked AST-to-IR lowering, RON/JSON serialization, and the `weavec` CLI | `weave-core`; never `weave-runtime` or Bevy |
| `weave-fmt` | Canonical `.weave` source rendering | `weave-core` |
| `weave-bevy` | Bevy asset loading, hot reload, ECS resources, commands, and observer events | `weave-core`, `weave-compiler`, and `weave-runtime` |
| `weave_editor` | Standalone GPUI editor delivered in Phase 3 | Public APIs of the crates above |

The dependency direction is:

```text
weave-core
├── weave-runtime
├── weave-patterns
├── weave-compiler
└── weave-fmt

weave-core + weave-compiler + weave-runtime
└── weave-bevy
```

`weave-core::ast` and `weave-core::ir` are deliberately separate models. The AST preserves author-facing syntax and source spans. The IR is a versioned, deterministic, runtime-facing schema and must not contain parser implementation types.

## Compatibility

- Workspace MSRV: Rust 1.89. This is the minimum supported by Bevy 0.18.1.
- Serialized IR version: `weave_core::ir::IR_VERSION`.
- Bevy integration target: Bevy 0.18.
- Phase 1 source files use the language contract in [`language_guide.md`](language_guide.md).

Changing source semantics requires updating the language guide and parser fixtures. Changing serialized IR requires an explicit version decision and compatibility tests. Runtime state is serialized separately from immutable story data.

## Quality gates

The workspace gate is:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

Focused crates add golden, property, and integration tests where their acceptance criteria require them.
