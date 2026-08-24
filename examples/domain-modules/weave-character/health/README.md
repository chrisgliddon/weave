# Character corpus health fixtures

This directory contains original MIT-licensed synthetic Weave data. Ari Vale, the malformed and unsafe cases, every policy choice, and every expected diagnostic were authored for Weave. The corpus is fictional, credential-free, and policy-neutral.

Each project has one strict `project.health-manifest.{json,ron}` and the exact equivalent `report.health-report.{json,ron,txt}`. Source paths are relative to the project directory, one Character collection is marked `primary`, and `logical_id` pairs semantically equivalent RON and JSON documents.

| Project | Expected result |
|---|---|
| `healthy/` | Complete profile and exact RON/JSON collection and runtime-pack pairs; no diagnostics and CI status `0` |
| `incomplete/` | One absent facet value; advisory `H104` and CI status `0` under the configured error threshold |
| `stale/` | Stale attributed data and fingerprints; `H106` diagnostics and CI status `2` |
| `unsafe/` | Restricted personalization placeholder; source-located `H304` and CI status `2` |
| `malformed/` | Missing required collection content and failed strict decoding; `H101`/`H100` and CI status `2` |
| `migration-required/` | Unsupported collection format; `H102` and CI status `2` |

The audit is byte-for-byte read-only. A report contains the manifest/input fingerprint, coverage, descriptive distributions, stable diagnostics, reviewed suppression results, and source locations, but never retains source payloads. Distribution counts are informational unless the manifest explicitly configures a documented constraint. CSV remains a review-only, potentially lossy export and is never canonical input.

Rebuild or verify all six projects and both schemas:

```bash
cargo run -p weave-character --example health_fixture -- --write
cargo run -p weave-character --example health_fixture -- --check
```

Run the healthy audit in all equivalent report formats:

```bash
cargo run -p weave-character -- health-audit \
  examples/domain-modules/weave-character/health/healthy/project.health-manifest.json

cargo run -p weave-character -- health-audit \
  examples/domain-modules/weave-character/health/healthy/project.health-manifest.json \
  --format json \
  --output target/character-health.json

cargo run -p weave-character -- health-audit \
  examples/domain-modules/weave-character/health/healthy/project.health-manifest.ron \
  --format ron \
  --output target/character-health.ron
```

Filter presentation without weakening the complete report's CI decision:

```bash
cargo run -p weave-character -- health-audit \
  examples/domain-modules/weave-character/health/unsafe/project.health-manifest.json \
  --format json \
  --code H304 \
  --minimum-severity error \
  --ci
```

`--ci` returns status `2` only when an active diagnostic meets the manifest's configured `failure_threshold`; ordinary command or manifest errors return status `1`. Diagnostic filters affect rendered output only. Reviewed suppressions are versioned, attributed, rationalized, and reported even when they match nothing.

The checked schemas are [`weave-character-health-manifest-v1.schema.json`](../../../../schemas/weave-character-health-manifest-v1.schema.json) and [`weave-character-health-report-v1.schema.json`](../../../../schemas/weave-character-health-report-v1.schema.json).
