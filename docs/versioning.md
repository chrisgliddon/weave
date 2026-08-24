# Version and compatibility

You are reading the documentation for **Weave 0.1.0**. The navigation title and version badge identify the language release described by every page in this build.

## Compatibility values

| Surface | Current value | Authority |
|---|---:|---|
| Language specification | `0.1.0` | [`language_guide.md`](language_guide.md) |
| Rust workspace packages | `0.1.0` | Root `Cargo.toml` |
| Serialized story IR | `4` | `weave_core::ir::IR_VERSION` |
| JSON Schema | IR `4` | [`json_format.md`](json_format.md) |
| Saved browser state | `1` | `weave_web::WEB_STATE_VERSION` |
| Pattern model | `1` | `weave_patterns::PATTERN_MODEL_VERSION` |
| Community pattern package | `1` | [`community_patterns.md`](community_patterns.md) |
| Community registry index | `1` | `weave_patterns::COMMUNITY_REGISTRY_INDEX_VERSION` |
| Domain module contract | `1` | `weave_domain::DOMAIN_CONTRACT_VERSION` |
| Domain pack format | `1` | `weave_domain::DOMAIN_PACK_FORMAT_VERSION` |
| Domain project file | `1` | `weave_domain::DOMAIN_PROJECT_FORMAT_VERSION` |
| Domain lock file | `1` | `weave_domain::DOMAIN_LOCK_FORMAT_VERSION` |
| Domain registry index | `1` | `weave_domain::DOMAIN_REGISTRY_INDEX_VERSION` |
| World composition plan/receipt | `1` | `weave_world::WORLD_COMPOSITION_FORMAT_VERSION` |
| Full/compact World export | `1` | `weave_world::WORLD_EXPORT_FORMAT_VERSION` |
| Character profile/template/overlay/synthesis | `1` | [`character_module.md`](character_module.md) |
| Character collection/request/proposal/review/progress | `1` | [`character_module.md`](character_module.md) |
| Character authoring workspace/revision/preview | `1` | [`character_module.md`](character_module.md) |
| Character questionnaire pack/answers/proposal/review/receipt | `1` | [`character_module.md`](character_module.md) |
| Character final review | `1` | [`character_module.md`](character_module.md) |
| Character assistance template/request/preview/approval/provider response/candidate set | `1` | [`character_module.md`](character_module.md) |
| Character assistance advisory/decision review/receipt/batch/comparison | `1` | [`character_module.md`](character_module.md) |
| Character projection pack/config/proposal/review/receipt/lock revision | `1` | [`character_module.md`](character_module.md) |
| Weave Character domain module | `1.6.0` | `weave_character::CHARACTER_DOMAIN_MODULE_VERSION` |
| Formatter contract | `1` | `weave_fmt::FORMAT_VERSION` |
| Bevy integration | Bevy `0.18` | `weave_bevy::BEVY_VERSION` |
| Tree-sitter package | `0.1.0` | [`tree_sitter.md`](tree_sitter.md) |

Consumers must check the explicit IR or saved-state value at the boundary where data is loaded. A matching crate version does not authorize guessing across an unsupported serialized version.

## Documentation releases

The site is rebuilt from the default branch by repository automation. Release documentation is tied to the matching Git tag; development documentation describes the current default branch and may include unreleased changes. Syntax changes update the normative language guide and Tree-sitter package version together.

When linking durable technical material, prefer a tagged source URL such as `https://github.com/chrisgliddon/weave/tree/v0.1.0/docs` once that tag exists. Use the Pages site for the current, searchable documentation.
