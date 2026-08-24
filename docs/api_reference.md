# Rust API entry points

The documentation build regenerates rustdoc for every public integration crate and publishes it under this site. Start at the narrowest package that owns your concern.

| Package | Primary entry points | Generated API |
|---|---|---|
| `weave-core` | `parse_document`, `parse_expression`, `type_check_with_extensions`, domain-module signatures, AST, diagnostics, and versioned IR | [Open rustdoc](api/weave_core/index.html) |
| `weave-compiler` | `compile`, `compile_with_extensions`, `compile_with_modules`, `to_ron`, `to_json`, `json_schema`, and `CompileOptions` | [Open rustdoc](api/weave_compiler/index.html) |
| `weave-runtime` | `Story`, `StoryEvent`, `StoryState`, `ChoiceView`, and `RuntimeError` | [Open rustdoc](api/weave_runtime/index.html) |
| `weave-patterns` | `PatternSystem`, built-ins, `CommunityPackage`, `PackageRegistry`, package validation/publication, and `system_from_ir` | [Open rustdoc](api/weave_patterns/index.html) |
| `weave-domain` | `ModuleManifest`, `DomainPack`, `ReadOnlyPathDeclaration`, `DomainCatalog`, `DomainRegistry`, `DomainProjectConfig`, `DomainLock`, schema generation, validation, discovery, and deterministic resolution | [Open rustdoc](api/weave_domain/index.html) |
| `weave-character` | `CharacterProfile`, all HEXACO factors/facets, derived OCEAN views, templates/overlays/synthesis, guided and assisted authoring, typed provider-neutral candidates, immutable relationship-kind packs, layered graph revisions/proposals/reviews/receipts/reconciliation, deterministic collections and batches, alignment/date-context receipts, `CharacterHealthManifest`, `audit_character_health`, stable health reports/filters/schemas, `character_module_manifest`, and `character_domain_pack` | [Open rustdoc](api/weave_character/index.html) |
| `weave-tabletop` | Adapter manifests, exact selection, isolated Character projections and mutable state, deterministic entropy, trusted resolver registry, typed visibility-scoped events, switching previews, source-license gate, and schemas | [Open rustdoc](api/weave_tabletop/index.html) |
| `weave-world` | `compose_world_pack`, versioned plans/receipts, full/compact exports and schemas, `resolve_world`, stable places, and recursive `WorldValueOrigin` lineage | [Open rustdoc](api/weave_world/index.html) |
| `weave-world-corpus` | `PresetSource`, `CorpusIndex`, `build_pack`, `process_corpus`, and corpus schema generation | [Open rustdoc](api/weave_world_corpus/index.html) |
| `weave-fmt` | `format_source` and `format_document` | [Open rustdoc](api/weave_fmt/index.html) |
| `weave-bevy` | `WeavePlugin`, `WeaveStory`, observer events, and `WeaveCommandsExt` | [Open rustdoc](api/weave_bevy/index.html) |
| `weave-web` | native `Player` plus WASM-facing player bindings | [Open rustdoc](api/weave_web/index.html) |
| `weave-lsp` | reusable LSP `Backend`; the `weave-lsp` binary owns stdio transport | [Open rustdoc](api/weave_lsp/index.html) |
| `tree-sitter-weave` | `LANGUAGE`, node types, and editor queries | [Open rustdoc](api/tree_sitter_weave/index.html) |

## Dependency rule

Use `weave-domain` for portable module contracts, `weave-character` for host-independent character profiles, immutable template/sparse authoring, provider-neutral assisted review, explainable classification and role projection packs, typed presentation catalogs, provenance-aware relationship graphs, deterministic synthesis, and read-only corpus health, `weave-tabletop` for optional ruleset capabilities/state/resolution without mutating Character canon, `weave-world-corpus` for offline environmental preset normalization, `weave-world` for deterministic pack composition plus host-independent authored-place inheritance/export, `weave-core` for source analysis, `weave-compiler` for checked lowering, and `weave-runtime` for execution. Pattern specialization stays behind `weave-patterns::PatternSystem`. Host integrations depend inward on those packages; contract and core code never depend on Bevy, GPUI, browser APIs, or editor protocols.

The [architecture guide](architecture.md) defines the complete allowed dependency direction. Serialized hosts should also read the [JSON compatibility contract](json_format.md).
