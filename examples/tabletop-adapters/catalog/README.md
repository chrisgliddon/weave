# Portable adapter catalog

This directory is the machine-readable index of every tabletop adapter shipped in this build.
Three files are generated from live adapter data. Do not edit them by hand.

| File | Purpose |
|---|---|
| `catalog.json` | Full adapter index: manifest facts, capabilities, creation steps, operations, source policy, conformance coverage, and the canonical project link. |
| `compatibility-matrix.json` | Compatibility and migration matrix: one adapter row, every directed cross-system pair, explicit non-conversion notes, and every declared same-adapter migration. |
| `upstream-snapshot-example/` | Synthetic worked example of the upstream snapshot record format. It names no real upstream work. |

## Read the catalog

| Field | Meaning |
|---|---|
| `catalog_format_version` | Catalog format. The current format is 1. |
| `id` | Stable globally namespaced adapter identity. |
| `version` | Adapter release version. |
| `schema_version` | Adapter state-schema version. |
| `namespace` | Adapter-owned namespace. It is always a segment of `id`. |
| `title`, `summary` | Human-readable adapter name and purpose. |
| `compatibility_label` | Factual compatibility name. It never implies endorsement. |
| `weave_version`, `character_contract_version` | Supported compatibility ranges. |
| `capabilities` | Declared capability ids and contract versions. |
| `creation_steps` | Ordered creation-step ids, titles, required capability, and field ids. |
| `operations` | Declared operation ids, required capability, event kinds, and entropy use. |
| `source` | Reviewed source boundary, licence expression, source class, digests, notices, exclusions, and the matching policy row. |
| `conformance.status` | `standard_suite_registered` or `not_registered`. |
| `conformance.checks` | Check ids the standard suite applies, or an empty list when unregistered. |
| `canonical_project` | Project path, exact reproduction command, expected-output path and digest, and seed. |
| `consumers` | Exact Bevy command for this adapter and the PixiJS suite command that covers every adapter. |
| `content_sha256` | Exact content hash of the validated manifest, so a type or visibility change moves the catalog row. |

Every adapter appears, including one outside the standard suite. That keeps the shipped surface
visible and lets a consumer check capabilities and licences before installing anything.

## Run a host consumer

Each adapter lists a `consumers.bevy` command, for example:

```bash
cargo run -p weave-example-bevy-tabletop -- spark
cargo run -p weave-example-bevy-tabletop -- wicked-ones
```

The PixiJS consumer covers every adapter from the same public projection:

```bash
cd examples/domain-module-pixijs && npm test
```

## Read the compatibility and migration matrix

`compatibility-matrix.json` is format 1 and contains:

- `adapters`: one row per adapter with version, state-schema version, capabilities, creation steps,
  operations, licence, source class, compatibility ranges, declared migration ids, the
  canonical project link, and the manifest content hash;
- `cross_system_pairs`: one row for every directed pair of different adapters. Every row sets
  `conversion_allowed` to false and names the five categories that never transfer: `attributes`,
  `items`, `resources`, `progression`, and `campaign_state`;
- `migrations`: one row for every same-adapter migration declared by a live manifest; and
- `conversion_policy`, `migration_policy`: the normative rules. Cross-system conversion requires a
  separately approved migration and is never automatic.

## Canonical synthetic projects

Each adapter has one canonical project under `examples/tabletop-adapters/<adapter>/project/`.
`project.json` selects one exact adapter coordinate and records the seed, synthetic source and
licence, and scripted operation path. `expected-output.json` records the expected creation
fingerprints and every receipt, state, entropy, and event result. The linked command compiles the
test harness, plays the scripted path, replays every receipt, and fails on any difference.

## Regenerate

The catalog and matrix are derived data. `crates/weave-tabletop/tests/catalog.rs` rebuilds both
from the live `*_manifest()` functions, `standard_source_policy()`, `standard_suite()`, and the
canonical project files. The normal tests fail when either checked-in file differs.

Regenerate only after the manifest, policy, or project change is reviewed:

```bash
WEAVE_WRITE_ADAPTER_CATALOG=1 cargo test -p weave-tabletop --test catalog -- --ignored
cargo test -p weave-tabletop --test catalog
```

Regenerate project output only after a reviewed adapter change:

```bash
WEAVE_WRITE_COMPATIBILITY_PROJECTS=1 cargo test -p weave-tabletop --test compatibility
cargo test -p weave-tabletop --test compatibility
```

## Licence boundary

Adapters that declare `CC0-1.0` or `MIT` remain on those terms. Open Tale declares
`Apache-2.0`, retains its full licence text and notices, and is never presented as MIT-only.
See `docs/tabletop_adapters.md` for the complete review rules and the upstream snapshot review
process.
