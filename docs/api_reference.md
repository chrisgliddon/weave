# Rust API entry points

The documentation build regenerates rustdoc for every public integration crate and publishes it under this site. Start at the narrowest package that owns your concern.

| Package | Primary entry points | Generated API |
|---|---|---|
| `weave-core` | `parse_document`, `parse_expression`, `type_check_with_extensions`, domain-module signatures, AST, diagnostics, and versioned IR | [Open rustdoc](api/weave_core/index.html) |
| `weave-compiler` | `compile`, `compile_with_extensions`, `compile_with_modules`, `to_ron`, `to_json`, `json_schema`, and `CompileOptions` | [Open rustdoc](api/weave_compiler/index.html) |
| `weave-runtime` | `Story`, `StoryEvent`, `StoryState`, `ChoiceView`, and `RuntimeError` | [Open rustdoc](api/weave_runtime/index.html) |
| `weave-patterns` | `PatternSystem`, built-ins, `CommunityPackage`, `PackageRegistry`, package validation/publication, and `system_from_ir` | [Open rustdoc](api/weave_patterns/index.html) |
| `weave-domain` | `ModuleManifest`, `DomainPack`, `DomainValue`, `DomainCatalog`, schema generation, validation, and deterministic resolution | [Open rustdoc](api/weave_domain/index.html) |
| `weave-fmt` | `format_source` and `format_document` | [Open rustdoc](api/weave_fmt/index.html) |
| `weave-bevy` | `WeavePlugin`, `WeaveStory`, observer events, and `WeaveCommandsExt` | [Open rustdoc](api/weave_bevy/index.html) |
| `weave-web` | native `Player` plus WASM-facing player bindings | [Open rustdoc](api/weave_web/index.html) |
| `weave-lsp` | reusable LSP `Backend`; the `weave-lsp` binary owns stdio transport | [Open rustdoc](api/weave_lsp/index.html) |
| `tree-sitter-weave` | `LANGUAGE`, node types, and editor queries | [Open rustdoc](api/tree_sitter_weave/index.html) |

## Dependency rule

Use `weave-domain` for portable module contracts, `weave-core` for source analysis, `weave-compiler` for checked lowering, and `weave-runtime` for execution. Pattern specialization stays behind `weave-patterns::PatternSystem`. Host integrations depend inward on those packages; contract and core code never depend on Bevy, GPUI, browser APIs, or editor protocols.

The [architecture guide](architecture.md) defines the complete allowed dependency direction. Serialized hosts should also read the [JSON compatibility contract](json_format.md).
