# Domain module in Bevy

This finite Bevy 0.18 example deserializes the canonical compiled RON tracer, Weave World
reference seed, and complete synthetic Weave Character profile. It extracts their typed exports
through `weave-core`, inserts the results as Bevy resources, and reports them from `Startup`
systems. The Character reader retains exact alignment pack/review/application fingerprints, reads
only reviewed/edited/overridden narrative labels, proves rejected and withheld axes are absent,
loads approved categorical, vocation, social-role, and narrative-role projections with exact
pack/proposal/review hashes, explanations, rationales, input paths, lossiness, decisions, and locks,
and reads attributed pronouns, palette accent, a safe avatar path, exact presentation catalog
id/hash, visual tone, and the presentation lock. A separate `ExpressionReading` resource loads the
normalized `trailmark` term, categorized preference, exact dialogue template/scenario, and pinned
expression-pack id/version/hash. It verifies every projection write-back target plus expression,
alignment, and presentation canonical personality write-back are false. It has no editor dependency
and performs no artifact discovery at runtime.

Run it from the repository root:

```bash
cargo run -p weave-example-domain-module-bevy
```

The same explicitly ordered `Startup` schedule also loads the generated Plug-And-Play and
Dungeonpunk stories and runtime-projected receipts. It reports each exact adapter coordinate,
portable character/state values, public check or Struggle consequence, request fingerprint, and
hidden entropy payload fingerprint. The entropy payloads themselves must remain redacted for the
runtime audience.

The process performs one Bevy update and exits, so the same command is suitable for documentation
and CI smoke checks.
