# Typed Character presentation fixtures

This directory is the public, offline reference workflow for identity-adjacent presentation data.
It demonstrates attributed pronouns, authored origin/context notes, appearance descriptors, a
portable palette, style tags, SVG avatars, deterministic catalog suggestions, complete review,
locks, author override, and atomic apply. None of these fields can infer or modify personality,
alignment, birth context, ruleset state, or protected identity characteristics.

## Corpus and provenance

The Glasswind catalog, Ari Vale and Sable Reed identities, prose, palette, style tags, allocation
decisions, and both SVG assets were authored for Weave and are distributed under the repository's
MIT license. `glasswind.presentation-catalog.{json,ron}` carries the exact source attribution,
license URL, authored revision, and claims for its slots, entries, and assets. The workflow is
finite and performs no provider lookup or network access.

Catalog entries declare only exact eligible character ids or stable id prefixes, explicit
exclusions, compatible value kinds, and optional capacities. The allocator never reads a profile
field to decide eligibility. The request pins the input collection hash, catalog id/version/hash,
seed `20260822`, sorted character and slot ids, and the complete available-asset inventory.

## Checked artifacts

- `input.character-collection.{json,ron}` contains direct authored presentation fields and no
  catalog assignments.
- `glasswind.presentation-catalog.{json,ron}` contains four closed slots and two capacity-one
  entries per slot.
- `allocation.presentation-request.{json,ron}` is the exact `fill_missing` allocation request.
- `proposal.presentation-proposal.{json,ron}` records balance counts and every eligibility,
  exclusion, capacity, prior-use, seed-hash, and selection trace.
- `decisions.presentation-review.{json,ron}` accepts the proposed values except for one explicit
  compatible Sable accent override and locks Ari's avatar.
- `review.presentation-review.{json,ron}` pins the complete proposal and editorial rationale.
- `receipt.presentation-receipt.{json,ron}` embeds replay inputs, sparse operations, hashes, and
  the exact output collection.
- `applied.character-collection.{json,ron}` is the atomic reviewed result.
- `unlock-avatar.presentation-lock-revision.{json,ron}` pins the applied collection and one exact
  character/slot target; `unlocked.character-collection.{json,ron}` is its deterministic result.
- `ari_vale.character.{json,ron}` is the typed profile used by the ordinary domain-module story,
  Bevy consumer, and PixiJS consumer.

The repository-level `invalid/` directory includes missing-asset, invalid-palette-slot,
incompatible-override, stale-proposal, and duplicate-alias inputs. They fail before any output is
applied and diagnostics do not echo rejected values.

## Reproduce offline

From the repository root:

```bash
cargo run -p weave-character --example character_fixture -- --check

cargo run -p weave-character -- presentation-propose \
  examples/domain-modules/weave-character/presentation/input.character-collection.json \
  examples/domain-modules/weave-character/presentation/glasswind.presentation-catalog.json \
  examples/domain-modules/weave-character/presentation/allocation.presentation-request.json \
  --output target/proposal.presentation-proposal.json

cargo run -p weave-character -- presentation-review \
  target/proposal.presentation-proposal.json \
  examples/domain-modules/weave-character/presentation/decisions.presentation-review.json \
  --reviewer org.weave.reviewer.presentation_fixture \
  --rationale "Review every synthetic presentation allocation; retain catalog coordinates, balance traces, locks, and explicit override rationale." \
  --output target/review.presentation-review.json

cargo run -p weave-character -- presentation-apply \
  examples/domain-modules/weave-character/presentation/input.character-collection.json \
  target/proposal.presentation-proposal.json \
  target/review.presentation-review.json \
  --receipt-output target/receipt.presentation-receipt.json \
  --collection-output target/applied.character-collection.json

cargo run -p weave-character -- presentation-lock \
  examples/domain-modules/weave-character/presentation/applied.character-collection.json \
  examples/domain-modules/weave-character/presentation/unlock-avatar.presentation-lock-revision.json \
  --output target/unlocked.character-collection.json
```

With the exact reviewer and rationale above, every generated JSON file matches its checked bytes.
Use the paired RON inputs with `--format ron` for the checked RON artifacts.
`presentation-apply --dry-run` still writes the exact receipt while omitting the collection;
`presentation-lock --dry-run` validates without writing either input or output.
`presentation-revision` can then adopt one character's accepted sparse operations into a matching
guided-authoring draft; the CLI, editor service, and Rust function produce identical revision and
workspace bytes.

See the [Character contract guide](../../../../docs/character_module.md#typed-identity-and-presentation-authoring)
for sparse operation syntax, field origins, migration rules, allocation policy, and runtime
projection details.
