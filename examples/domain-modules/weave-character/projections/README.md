# Explainable Character projection fixtures

Glasswind Lenses is an original MIT-licensed public example pack for optional fictional writing prompts. It defines one deliberately lossy categorical personality display plus vocation, social-role, and narrative-role suggestions. The labels, descriptions, weights, calibration vectors, methodology, limitations, and expected outputs were authored for Weave.

The pack accepts only declared canonical HEXACO factor/facet paths. For every present input, it converts the normalized value to integer millionths, centers it with `2 × value - 1_000_000`, multiplies by the declared signed weight, and divides the sum by covered absolute weight using deterministic half-away-from-zero rounding. Missing inputs reduce coverage and contribute neither a value nor imputed precision. A candidate qualifies only when the declared character eligibility, exclusion, coverage, and signed-score thresholds all pass.

Qualifying candidates are ordered by score descending, then by a SHA-256 tie-break over the exact pack coordinate, configuration hash, character id, taxonomy id, entry id, seed, and evidence trace, then by stable id. Reservations are assigned first. Existing authored, overridden, or locked values are retained. Remaining assignments respect per-entry capacity; rebalance mode can reconsider only unlocked reviewed, edited, or derived values. Every proposal embeds the exact collection, pack, configuration, seed, candidate evidence, distribution counts, and complete review manifest, so replay does not depend on a registry, clock, provider, editor, or network.

The checked artifacts are:

- `glasswind.projection-pack.{json,ron}`: installable pack, license/provenance, declared inputs, four taxonomies, signed scoring rules, thresholds, capacities, limitations, and two complete calibration vectors;
- `selection.projection-config.{json,ron}`: three-character eligible pool, explicit per-entry capacities, one reservation, and fill-missing policy;
- `proposal.projection-proposal.{json,ron}`: deterministic batch ranking, complete evidence/seed/capacity traces, distribution statistics, and review manifest;
- `decisions.projection-review.{json,ron}` and `review.projection-review.{json,ron}`: complete accept, edit, override, reject, withhold, and lock choices with rationales;
- `receipt.projection-receipt.{json,ron}` and `applied.character-collection.{json,ron}`: independently replayable atomic output with exact pack/proposal/review lineage;
- `unlock.projection-lock-revision.{json,ron}` and `unlocked.character-collection.{json,ron}`: explicit lock revision; and
- `rebalance.projection-config.{json,ron}` plus `rebalance.projection-proposal.{json,ron}`: a new seed and rebalance that preserves the accepted editorial override.

Categorical labels are stored as `derived`, `lossy: true`, and `independent_evidence: false`. Role and vocation results become public only after an explicit accept, edit, or override. Reject and withhold publish no value. Every approved pack value retains its label, explanation, rationale, input paths, score/coverage when applicable, pack id/version/hash, proposal hash, review hash, decision, and lock.

Projection application owns only `org.weave.character.role_projections`. It verifies that canon, derived OCEAN, suggestions, alignment, identity presentation, expression, relationships, temporal context, and ruleset extensions remain byte-identical. The runtime domain projection repeats the boundary as an all-false `write_back` object for `hexaco`, `ocean`, `alignment`, `identity`, `birth`, `relationships`, and `ruleset`.

Rebuild or check the fixtures:

```bash
cargo run -p weave-character --example projection_fixture -- --write
cargo run -p weave-character --example projection_fixture -- --check
```

Reproduce and inspect the initial proposal:

```bash
cargo run -p weave-character -- projection-propose \
  examples/domain-modules/weave-character/projections/input.character-collection.json \
  examples/domain-modules/weave-character/projections/glasswind.projection-pack.json \
  examples/domain-modules/weave-character/projections/selection.projection-config.json \
  --seed 20260824 \
  --output target/proposal.projection-proposal.json

cargo run -p weave-character -- projection-inspect \
  target/proposal.projection-proposal.json \
  org.weave.character.ari_vale \
  org.weave.projection.glasswind_lenses.narrative_role
```

Create the complete review and atomically apply it:

```bash
cargo run -p weave-character -- projection-review \
  target/proposal.projection-proposal.json \
  examples/domain-modules/weave-character/projections/decisions.projection-review.json \
  --reviewer org.weave.reviewer.fixture \
  --rationale "Review every original Glasswind Lenses display and role independently; retain only explicit fictional editorial choices outside canon." \
  --output target/review.projection-review.json

cargo run -p weave-character -- projection-apply \
  examples/domain-modules/weave-character/projections/input.character-collection.json \
  target/proposal.projection-proposal.json \
  target/review.projection-review.json \
  --receipt-output target/receipt.projection-receipt.json \
  --collection-output target/applied.character-collection.json
```

Use `--dry-run` on apply or lock commands to reproduce and validate the transition without replacing a collection. JSON and RON commands share the same functions as `ProjectionSession` in the Weave Editor.
