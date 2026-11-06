# Portable adapter catalog

`catalog.json` is the machine-readable index of every tabletop adapter shipped in this build.
The file is generated from the live Rust manifests and reviewed source-policy rows. Do not edit it
by hand.

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

Every adapter appears, including one outside the standard suite. That keeps the shipped surface
visible and lets a consumer check capabilities and licences before installing anything.

## Regenerate

The catalog is derived data. `crates/weave-tabletop/tests/catalog.rs` rebuilds it from
`*_manifest()`, `standard_source_policy()`, and `standard_suite()`. The normal test fails when the
checked-in file differs, so a changed capability, operation, licence, source digest, or conformance
registration cannot enter a release silently.

Regenerate only after the manifest and policy change is reviewed:

```bash
WEAVE_WRITE_ADAPTER_CATALOG=1 cargo test -p weave-tabletop --test catalog -- --ignored
cargo test -p weave-tabletop --test catalog
```

The first command rewrites `catalog.json`. The second command proves the file matches live data.

## Licence boundary

Adapters that declare `CC0-1.0` or `MIT` remain on those terms. Open Tale declares
`Apache-2.0`, retains its full licence text and notices, and is never presented as MIT-only.
See `docs/tabletop_adapters.md` for the complete review rules.
