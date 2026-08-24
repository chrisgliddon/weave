# Weave Character contract fixtures

This directory contains original MIT-licensed synthetic data for the public Character Profile v1 contract. Ari Vale, Sable Reed, Glasswind, all prose, all numeric values, every optional extension payload, and every expected result were authored for Weave. The fixture uses the publicly documented HEXACO factor/facet vocabulary but contains no questionnaire items, scoring keys, normative data, private person data, or model-generated biography.

The canonical pairs are:

- `profile.character.{json,ron}`: complete six-factor and 24-facet profile;
- `template.character.{json,ron}`: immutable versioned input;
- `overlay.character.{json,ron}`: sparse reviewed operations with exact prior hashes;
- `synthesis.character.{json,ron}`: reproducible template/overlay/effective-profile proof;
- `omitted-extensions.character.{json,ron}`: valid minimal optional-domain behavior; and
- `unknown-extension-preserved.character.{json,ron}`: inactive opaque version preservation.
- `module.weave-module.{json,ron}` and `ari_vale.weave-domain.{json,ron}`: the shared declarative module boundary and immutable runtime projection; and
- `ari-vale.weave` plus `ari-vale.story.{json,ron}`: concise activation, one presentation-facing source override, typed trait reads, and exact compiled IR.
- `operations/collection.character-collection.{json,ron}`: a two-profile collection with stable cross-references;
- `operations/rename.character-{request,progress,proposal,review}.{json,ron}`: one pinned, resumable, whole-proposal rename workflow; and
- `operations/renamed.character-collection.{json,ron}`: the exact atomically applicable result.
- `context/`: three offline temporal packs, deterministic ranking configuration, complete proposal/review/receipt pairs, enriched output, and a separately locked runtime projection with fact/cue lineage split.
- `alignment/`: an original pluggable alignment pack, exact project selection, complete five-action review, independently reproducible receipt, and approved-only output profile.
- `projections/`: the original Glasswind Lenses pack, fixed-point classification and role scoring, capacity/reservation/eligible-pool batch allocation, complete evidence traces, all review actions, locks, and deterministic rebalance.
- `presentation/`: attributed identity presentation, an original catalog and SVG assets, deterministic balanced allocation, complete review, explicit override, lock/unlock, and atomic output.
- `expression/`: original normalized lexicon, preferences, vocabulary pools, behavioral signatures, voice constraints, a reusable dialogue template, exact pack assignment, lint/coverage reports, contextual resolution, and a stable fallback.

The `invalid/` directory covers unsupported profile and typed-extension versions, duplicate aliases and overlay targets, missing presentation assets, invalid palette slots, incompatible presentation overrides, stale template/review/progress/presentation/alignment/temporal lineage, incomplete alignment and temporal reviews, a malformed proposal, a broken relationship reference, and forbidden derived canonical evidence. `expression/invalid/` adds a restricted placeholder with an exact source-located, redaction-safe diagnostic.

Rebuild or check every fixture and schema from the repository root:

```bash
cargo run -p weave-character --example character_fixture -- --write
cargo run -p weave-character --example character_fixture -- --check
cargo run -p weave-character --example expression_fixture -- --write
cargo run -p weave-character --example expression_fixture -- --check
cargo run -p weave-character --example projection_fixture -- --write
cargo run -p weave-character --example projection_fixture -- --check
```

Validate or reproduce individual artifacts:

```bash
cargo run -p weave-character -- validate profile \
  examples/domain-modules/weave-character/profile.character.json

cargo run -p weave-character -- alignment-propose \
  examples/domain-modules/weave-character/alignment/input.character.json \
  --pack examples/domain-modules/weave-character/alignment/wayfinder_compass.alignment-pack.json \
  --config examples/domain-modules/weave-character/alignment/selection.alignment-config.json \
  --seed 20260822 \
  --output target/proposal.alignment-proposal.json

cargo run -p weave-character -- context-propose \
  examples/domain-modules/weave-character/context/input.character.json \
  --pack examples/domain-modules/weave-character/context/apollo_11.temporal-pack.json \
  --pack examples/domain-modules/weave-character/context/calendar.temporal-pack.json \
  --pack examples/domain-modules/weave-character/context/world.temporal-pack.json \
  --config examples/domain-modules/weave-character/context/ranking.temporal-config.json \
  --seed 19690720 \
  --output target/proposal.temporal-proposal.json

cargo run -p weave-character -- presentation-propose \
  examples/domain-modules/weave-character/presentation/input.character-collection.json \
  examples/domain-modules/weave-character/presentation/glasswind.presentation-catalog.json \
  examples/domain-modules/weave-character/presentation/allocation.presentation-request.json \
  --output target/proposal.presentation-proposal.json

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

cargo run -p weave-character -- collection-propose \
  examples/domain-modules/weave-character/operations/collection.character-collection.json \
  examples/domain-modules/weave-character/operations/rename.character-request.json \
  --output target/rename.character-proposal.json

cargo run -p weave-character -- collection-apply \
  examples/domain-modules/weave-character/operations/collection.character-collection.json \
  examples/domain-modules/weave-character/operations/rename.character-proposal.json \
  examples/domain-modules/weave-character/operations/rename.character-review.json \
  --dry-run

cargo run -p weave-character -- synthesize \
  examples/domain-modules/weave-character/overlay.character.json \
  --template examples/domain-modules/weave-character/template.character.json \
  --output target/synthesis.character.json

cargo run -p weave-character -- expression-validate \
  examples/domain-modules/weave-character/expression/applied.character.json \
  --pack examples/domain-modules/weave-character/expression/glasswind.expression-pack.json

cargo run -p weave-character -- expression-resolve \
  examples/domain-modules/weave-character/expression/applied.character.json \
  examples/domain-modules/weave-character/expression/glasswind.expression-pack.json \
  examples/domain-modules/weave-character/expression/contextual.expression-resolution-request.json \
  --output target/contextual.expression-resolution.json

cargo run -p weave-character -- domain-pack \
  examples/domain-modules/weave-character/profile.character.json \
  --id ari_vale \
  --version 1.0.0 \
  --title "Ari Vale Synthetic Character" \
  --output target/ari_vale.weave-domain.json

cargo run -p weave-compiler -- \
  examples/domain-modules/weave-character/ari-vale.weave \
  --locked \
  --format json \
  --output target/ari-vale.story.json
```

See the [projection fixture guide](projections/README.md), [expression fixture guide](expression/README.md), [presentation fixture guide](presentation/README.md), [alignment fixture guide](alignment/README.md), and [temporal-context fixture guide](context/README.md) for exact public provenance and reviewed reproduction commands. See the [Character contract guide](../../../docs/character_module.md) for authority, missing data, reusable expression/dialogue, typed presentation, explainable projections, OCEAN, alignment, temporal matching/ranking/review, extension, synthesis, compatibility, privacy, and diagnostic rules.
