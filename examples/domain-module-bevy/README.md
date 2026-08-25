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

The same explicitly ordered `Startup` schedule also loads the generated Plug-And-Play,
Dungeonpunk, and Freehack stories and receipts. For Freehack, Bevy acts as an authority host: it
validates the complete authority wrapper and the structurally separate public receipt, presents
only public story/check values, and reports merely that the private audit is present rather than
logging hidden opposition, signed draws, or entropy.

The process performs one Bevy update and exits, so the same command is suitable for documentation
and local smoke checks.
