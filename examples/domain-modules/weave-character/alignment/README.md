# Explainable alignment-view fixtures

This directory is the public, offline reference workflow for optional Character alignment views.
It demonstrates how an independently distributed, data-only pack can turn already-authored
canonical HEXACO evidence into transparent fictional storytelling shorthand. Alignment labels are
not diagnoses, psychometric results, moral rankings, protected-class inferences, predictions about
real people, canonical personality evidence, or runtime authority.

## Corpus and provenance

`wayfinder_compass.alignment-pack.{json,ron}` is wholly original MIT-licensed Weave material. Its
five axes, labels, prose, weights, thresholds, fixed-point methodology, limitations, and synthetic
calibration fixtures are attributed to the `weave_wayfinder_compass` source. It contains no copied
questionnaire items, scoring keys, normative samples, private data, or external corpus material.

The pack demonstrates the `domain_module` provider form by pinning:

- module `org.weave.alignment.wayfinder_compass@1.0.0`;
- provider pack `reference@1.0.0`; and
- the SHA-256 of the exact methodology, limitations, inputs, axes, and calibrations.

The provider is only a provenance coordinate. Proposal, review, apply, editor inspection, and
runtime consumption operate entirely on the checked data files and never load provider code or
contact a registry. A contributor can instead declare the `standalone` provider and use the same
contract without changing `weave-character`.

## Workflow artifacts

- `input.character.{json,ron}`: exact immutable Character profile before alignment;
- `wayfinder_compass.alignment-pack.{json,ron}`: complete pack rules and synthetic calibration;
- `selection.alignment-config.{json,ron}`: selected axes, minimum evidence coverage, and locked-view policy;
- `proposal.alignment-proposal.{json,ron}`: profile, pack, config, seed, missing-input coverage, scores, thresholds, and ordered explanation traces;
- `decisions.alignment-review.{json,ron}`: accept, edit, reject, override, and withhold decisions, each keyed by axis id;
- `review.alignment-review.{json,ron}`: complete review pinned to the exact proposal, profile, and pack;
- `receipt.alignment-receipt.{json,ron}`: independently replayable inputs, review, fingerprints, and atomic output; and
- `approved.character.{json,ron}`: public result containing only the accepted, edited, and overridden values.

The reviewed public view contains `horizon`, `reciprocity`, and `structure`. The rejected `signal`
and withheld `tempo` axes remain visible in the authoring receipt but are absent from the profile,
domain pack, Story IR, Bevy resource, and PixiJS presentation.

## Deterministic method

For each selected axis, the implementation converts present canonical evidence to integer
millionths, centers it around `500000`, applies the pack's signed thousandth weights, and divides
the signed sum by covered absolute weight with half-away-from-zero rounding. Coverage is computed
separately against total declared absolute weight. Missing evidence contributes no value and no
fabricated precision. The first inclusive threshold assigns the proposed neutral label.

The seed is pinned and hashed into every explanation trace but does not change v1 scores. Changing
the profile, pack, provider content, configuration, seed, selected-axis order, input trace, proposal,
or review changes a fingerprint or makes the artifact stale. Apply reproduces all inputs before
performing one atomic profile transition and never mutates `canon`.

## Reproduce offline

From the repository root:

```bash
cargo run -p weave-character --example character_fixture -- --check

cargo run -p weave-character -- alignment-propose \
  examples/domain-modules/weave-character/alignment/input.character.json \
  --pack examples/domain-modules/weave-character/alignment/wayfinder_compass.alignment-pack.json \
  --config examples/domain-modules/weave-character/alignment/selection.alignment-config.json \
  --seed 20260822 \
  --output target/proposal.alignment-proposal.json

cargo run -p weave-character -- alignment-review \
  target/proposal.alignment-proposal.json \
  --pack examples/domain-modules/weave-character/alignment/wayfinder_compass.alignment-pack.json \
  examples/domain-modules/weave-character/alignment/decisions.alignment-review.json \
  --reviewer org.weave.reviewer.fixture \
  --rationale "Review every original Wayfinder Compass axis independently; publish only approved fictional shorthand and never treat a label as diagnosis, moral rank, canonical evidence, or runtime authority." \
  --output target/review.alignment-review.json

cargo run -p weave-character -- alignment-apply \
  examples/domain-modules/weave-character/alignment/input.character.json \
  --pack examples/domain-modules/weave-character/alignment/wayfinder_compass.alignment-pack.json \
  target/proposal.alignment-proposal.json \
  target/review.alignment-review.json \
  --output target/receipt.alignment-receipt.json
```

With the exact reviewer and rationale above, all three outputs reproduce the checked JSON bytes.
Use `--format ron` with the RON inputs for the checked RON artifacts. `alignment-apply --dry-run`
validates and fingerprints the same receipt without creating an output file.

See the [Character contract guide](../../../../docs/character_module.md) for the full pack contract,
review semantics, editor integration, runtime projection, failure policy, and contributor workflow.
