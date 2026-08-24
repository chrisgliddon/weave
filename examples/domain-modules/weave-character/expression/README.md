# Weave Character expression fixtures

This directory is the complete offline fixture for reusable character expression and dialogue.
Every term, phrase, preference, behavioral signature, voice constraint, template, dialogue line,
scenario, request, and expected result was written for Weave and is available under the repository's
MIT license. This original fixture set is the sole source for its dialogue text. The five canonical
template lines are preserved in `glasswind-arrival.txt` so diagnostics
can report stable source, line, and column coordinates without echoing rejected text.

`glasswind.expression-pack.{json,ron}` is the immutable
`org.weave.expression.glasswind@1.0.0` pack. Its canonical SHA-256 is
`cb9a577720dde087af5e633c337dcb9814b12b25a0102636cc2a06444b8d66f6`. The pack declares its
eligibility, provenance, license, limitations, minimum coverage, and exact source ids. Assigning it
is explicit and fingerprinted; merely loading or validating the pack never changes a profile.

The checked artifact pairs cover:

- raw and canonicalized direct revisions, plus the atomically revised profile;
- an exact pack assignment request, replayable receipt, and applied profile;
- deterministic list/show data embedded in that profile, lint, and coverage reports;
- contextual resolution through personality, relationship, date-context, and World predicates;
- an authored relationship-specific winner and a stable authored fallback; and
- a deliberately restricted placeholder with a source-located, redaction-safe lint result.

Authored and accepted reviewed variants are eligible at runtime; unreviewed suggestions are not.
Selection sorts by authority, declared priority, and a SHA-256 seed tie-breaker. All applicability
predicates must match. If no contextual variant can be used, the declared fallback must be authored
or accepted, context-free, and resolvable from required public values. Substitution is limited to
the closed typed allowlist in the pack: speaker display name/pronoun, listener display name,
relationship kind, date label, World place name, and explicitly linked lexicon terms. There is no
arbitrary variable lookup, service call, generation model, or network access.

Rebuild or check every fixture and expression schema from the repository root:

```bash
cargo run -p weave-character --example expression_fixture -- --write
cargo run -p weave-character --example expression_fixture -- --check
```

Run the complete text workflow:

```bash
cargo run -p weave-character -- expression-normalize \
  examples/domain-modules/weave-character/expression/raw.expression-revision.json \
  --output target/normalized.expression-revision.json

cargo run -p weave-character -- expression-revise \
  examples/domain-modules/weave-character/omitted-extensions.character.json \
  examples/domain-modules/weave-character/expression/normalized.expression-revision.json \
  --output target/revised.character.json

cargo run -p weave-character -- expression-assign \
  examples/domain-modules/weave-character/expression/revised.character.json \
  examples/domain-modules/weave-character/expression/glasswind.expression-pack.json \
  examples/domain-modules/weave-character/expression/assignment.expression-request.json \
  --receipt-output target/assignment.expression-receipt.json \
  --profile-output target/applied.character.json

cargo run -p weave-character -- expression-list \
  examples/domain-modules/weave-character/expression/applied.character.json \
  --kind term --scenario arrival

cargo run -p weave-character -- expression-show \
  examples/domain-modules/weave-character/expression/applied.character.json \
  term trailmark

cargo run -p weave-character -- expression-lint \
  examples/domain-modules/weave-character/expression/applied.character.json \
  --pack examples/domain-modules/weave-character/expression/glasswind.expression-pack.json

cargo run -p weave-character -- expression-coverage \
  examples/domain-modules/weave-character/expression/applied.character.json \
  --pack examples/domain-modules/weave-character/expression/glasswind.expression-pack.json

cargo run -p weave-character -- expression-validate \
  examples/domain-modules/weave-character/expression/applied.character.json \
  --pack examples/domain-modules/weave-character/expression/glasswind.expression-pack.json

cargo run -p weave-character -- expression-resolve \
  examples/domain-modules/weave-character/expression/applied.character.json \
  examples/domain-modules/weave-character/expression/glasswind.expression-pack.json \
  examples/domain-modules/weave-character/expression/contextual.expression-resolution-request.json \
  --output target/contextual.expression-resolution.json
```

The contextual fixture resolves exactly to
`Ari Vale greets Tavi Quill at Stormwatch Gate and names their org.weave.relationship.friend a trailmark.`
The no-context fixture resolves exactly to `Ari Vale checks the route.`

Expression diagnostics use `X100` through `X112` for empty or over-limit text, unresolved tokens,
conflicting constraints, unsafe personalization, unreviewed suggestions, invalid links, duplicate
or near-duplicate variants, invalid or unavailable placeholders, stale packs, and invalid structure.
Messages are fixed and never reproduce rejected author text. See the
[Character contract guide](../../../../docs/character_module.md) for the full data and runtime
contract.
