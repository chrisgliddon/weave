# Weave Architecture

This document defines package ownership and dependency boundaries for the Rust workspace. The boundaries are part of the public architecture: new dependencies must not create cycles or move source-language concerns into integration crates.

## Packages

| Package | Responsibility | May depend on |
|---|---|---|
| `weave-domain` | Host-independent domain-module contracts, typed read-only paths, immutable registry packaging, bounded project discovery, exact locks, deterministic resolution, and provenance | General-purpose serialization, hashing, schema, semantic-version, and URL crates only |
| `weave-character` | Versioned Character Profile/collection/guided-and-assisted-authoring schemas, canonical evidence boundaries, typed identity presentation, deterministic catalog and assistance batch review, original questionnaire scoring, scoped corpus operations, immutable template synthesis/migration, read-only corpus health, and validated domain-pack projection | `weave-domain` plus general-purpose CLI, hashing, schema, semantic-version, and serialization crates; never parser, compiler, runtime, editor, network-client, provider-SDK, or engine APIs |
| `weave-tabletop` | Exact optional adapter selection, isolated definitions/state, generic creation/capability discovery, deterministic resolver registry, typed events/visibility, switch previews, migrations, and source-license policy | `weave-domain` plus general-purpose CLI, hashing, schema, semantic-version, and serialization crates; never parser, compiler, runtime, editor, network, provider, or engine APIs |
| `weave-world` | Deterministic World pack composition, versioned full/compact exports, stable fictional-place resolution, environment/climate inheritance, and recursive value lineage | `weave-domain` plus general-purpose CLI, hashing, schema, semantic-version, and serialization crates; never parser, compiler, runtime, editor, network, or engine APIs |
| `weave-world-corpus` | Versioned offline normalization of reviewed climate and environmental source records into canonical Weave World packs | `weave-domain` plus general-purpose CLI, schema, semantic-version, and serialization crates; never parser, compiler, runtime, or network clients |
| `weave-core` | Source AST, spans, diagnostics, parser, static analysis, and versioned runtime IR | `weave-domain` plus general-purpose parsing and serialization crates |
| `weave-runtime` | Deterministic execution of compiled stories and saveable story/pattern state | `weave-core`, `weave-patterns`; never Bevy |
| `weave-patterns` | Serializable pattern extension boundary, data-only community package registry, and built-in tarot, I-Ching, and Elder Futhark data/algorithms | `weave-core`; never compiler, runtime, or host APIs |
| `weave-compiler` | Checked AST-to-IR lowering, external package and domain-module embedding, RON/JSON serialization, and the `weavec` CLI | `weave-core`, `weave-patterns`, and `weave-domain`; never `weave-runtime` or Bevy |
| `weave-fmt` | Canonical `.weave` source rendering | `weave-core` |
| `weave-bevy` | Bevy asset loading, hot reload, ECS resources, commands, and observer events | `weave-core`, `weave-compiler`, and `weave-runtime` |
| `weave-web` | Browser-safe JSON loading, deterministic playback, and versioned save-state bindings | `weave-core`, `weave-patterns`, and `weave-runtime`; never Bevy or desktop APIs |
| `weave-lsp` | Editor-independent diagnostics, navigation, completion, rename, hover, and formatting over LSP stdio | `weave-core` and `weave-fmt`; never Bevy, GPUI, or editor APIs |
| `tree-sitter-weave` | Incremental concrete-syntax parser plus highlight, local-variable, and symbol-tag queries for downstream editors | Generated C parser with language bindings; no dependency on the Weave runtime or editor |
| `weave_editor` | Standalone GPUI editor and explicit domain-module inspection session | Public APIs of the crates above |

The dependency direction is:

```text
weave-domain
├── weave-character
├── weave-tabletop
├── weave-world
├── weave-world-corpus
└── weave-core
    ├── weave-patterns
    └── weave-fmt

weave-core + weave-domain + weave-patterns
└── weave-compiler

weave-core + weave-patterns
└── weave-runtime

weave-core + weave-patterns + weave-runtime
└── weave-web

weave-core + weave-fmt
└── weave-lsp

Weave language specification
└── tree-sitter-weave

weave-core + weave-patterns + weave-compiler + weave-runtime
└── weave-bevy
```

`weave-domain` is deliberately independent of the language parser, compiler, runtime, editor, and host integrations. Contract v1 is declarative and cannot load package-defined code. It owns immutable registry layout, checksum verification, project-relative discovery, dependency closure, and exact locks as well as portable values. `weave-core` consumes those value types; the compiler resolves a bounded `DomainCatalog` and lowers selected effective values plus authored provenance into IR 4; runtime, editor, Bevy, and browser boundaries consume that shared representation rather than defining host-specific module models. The complete contract, packaging, activation, compatibility, provenance, and tracer rules are in the [domain-module guide](domain_modules.md).

`weave-world-corpus` is a domain-specific authoring tool above that contract. It reads only versioned local records, applies closed unit, approximation, daylight, and hazard policies, and asks `weave-domain` to validate its generated packs. It does not acquire data, parse `.weave`, execute stories, or add World concepts to the shared compiler/runtime boundary. `weave-world` composes reviewed pack fields into another ordinary pack, records reproducible layer decisions, emits full/compact resolved projections, and resolves fictional place environment/climate inheritance. Its library remains free of source parsing, I/O, network access, and host dependencies; file handling belongs to its finite CLI and fixture example.

`weave-character` is another domain-specific contract above `weave-domain`. It validates profiles, evidence state, extension isolation, exact lineage, immutable template fingerprints, sparse operations, effective field origins, typed presentation assets, deterministic catalog eligibility/balance/locks, explainable fixed-point classification and role packs with capacity/reservation/rebalance review, reproducible derived views, guided workspaces, fixed-point questionnaire proposals, explicit conflict/final reviews, provider-neutral assistance templates/previews/candidates/reviews, and deterministic collection or assistance-batch receipts, then projects a validated profile into an ordinary declarative manifest and pack. The health router composes those same validators over an explicit in-memory source map, returning only stable diagnostics, hashes, locations, coverage, and descriptive distributions; filesystem loading/output stays in the finite CLI, and the core audit cannot mutate source bytes. Its assistance trait is only a credential-status and payload/response boundary: the crate supplies a complete offline implementation and contains no network client or provider SDK. The editor depends inward on this crate for `CharacterCorpusSession`, `CharacterAuthoringSession`, `CharacterAssistanceSession`, `CharacterHealthSession`, `CharacterPresentationSession`, `ProjectionSession`, alignment, and temporal sessions; it does not duplicate mutation rules. Compiler, Bevy, and PixiJS consumers continue to use declarative output. File persistence and optional adapter wiring belong to the finite CLI/editor host; the contract library remains independent of source parsing, network access, runtime, and engine APIs.

`weave-tabletop` remains beside rather than inside `weave-character`. It references canonical Character data only by stable id and fingerprint, owns all adapter definitions and mutable rules state, and forbids canonical write-back. Distributed packages are declarative. Trusted hosts may explicitly compile and register an implementation of the exact resolver trait; no package can load code. The registry owns validation, deterministic entropy, event visibility, and replay around each call, while compiler/runtime and engine integrations consume ordinary portable projections.

`tree-sitter-weave` is intentionally independent of `weave-core`: editors can parse unfinished source without linking the compiler. Its grammar and queries must track the normative language guide, and its npm package, Rust crate, grammar metadata, and language-specification version are released together.

`weave-core::ast` and `weave-core::ir` are deliberately separate models. The AST preserves author-facing syntax and source spans. The IR is a versioned, deterministic, runtime-facing schema and must not contain parser implementation types.

## Compatibility

- Workspace MSRV: Rust 1.93. The editor's pinned GPUI revision establishes this minimum; Bevy 0.18.1 remains supported.
- Serialized IR version: `weave_core::ir::IR_VERSION`.
- Bevy integration target: Bevy 0.18.
- Source files use the versioned language contract in [`language_guide.md`](language_guide.md).

Changing source semantics requires updating the language guide and parser fixtures. Changing serialized IR requires an explicit version decision and compatibility tests; the [JSON format contract](json_format.md) and its checked-in schema are authoritative for interoperable hosts. `PatternDefinition` and `StoryIr` are immutable/shareable. `PatternState` is caller-owned, serialized inside `StoryState`, and reconciled against the current definition on restore. The runtime owns only trait objects and never switches on Tarot, I-Ching, or rune identities; specialization lives behind `weave-patterns::PatternSystem`.

Community package parsing, compatibility, checksums, publication, installation, and discovery live in `weave-patterns`. `weavec` resolves explicitly selected installed packages, supplies their spread signatures to static analysis, and embeds their validated data into ordinary IR. Neither the compiler nor runtime downloads packages, loads dynamic libraries, or executes package-defined hooks.

## Quality gates

The workspace gate is:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
```

Focused crates add golden, property, and integration tests where their acceptance criteria require them.
