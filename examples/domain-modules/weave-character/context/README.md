# Reviewed temporal-context fixtures

This directory is the public, offline reference corpus for Character date-context enrichment. It demonstrates how an explicit fictional birth-date precision can surface authoring cues without claiming that a date, place, calendar fact, environmental condition, or historical event causes or predicts personality.

The contract keeps two kinds of material machine-separate:

- a structured fact with its own public or original source ids; and
- an original fictional cue plus its declared HEXACO ranking vector with different source ids.

Ranking reads the cue vector and the character's already-authored evidence. It never reads the fact payload. Accepted material remains in the `org.weave.character.date_context` extension and pending suggestions with canonical personality write-back forbidden.

## Corpus and provenance

`apollo_11.temporal-pack.{json,ron}` pins the public Wikidata Apollo 11 entity (`Q43653`) at revision `2526069885`, modified `2026-08-02T06:29:38Z`:

- artifact: <https://www.wikidata.org/wiki/Special:EntityData/Q43653.json?revision=2526069885&flavor=simple>
- artifact SHA-256: `52c78c8a7c1320e970a6aa1c2737c9e0a89e786be957f380f389d02ab84c006b`
- data access: <https://www.wikidata.org/wiki/Help:Data_access>
- license: [Wikidata CC0](https://www.wikidata.org/wiki/Wikidata:Licensing)

Only the structured event date is sourced from Wikidata. The fictional prompt and declared vector are original MIT-licensed Weave material under `weave_historical_cues`; they are not paraphrases of Wikidata text.

`calendar.temporal-pack.{json,ron}` contains original MIT-licensed calendar and seasonal fixtures. `world.temporal-pack.{json,ron}` demonstrates an optional, content-hashed `domain_module` provider projection from Weave World. It is a checked offline input, not a runtime dependency.

## Workflow artifacts

- `input.character.{json,ron}`: exact immutable input profile with a full `1969-07-20` date;
- the three `*.temporal-pack.{json,ron}` pairs: standalone and World-compatible context providers;
- `ranking.temporal-config.{json,ron}`: filters, fixed-point thresholds, place/time-zone scope, candidate limit, seed policy, and disabled auto-approval;
- `proposal.temporal-proposal.{json,ron}`: four ranked candidates plus complete selected, downgraded, and skipped coverage;
- `decisions.temporal-review.{json,ron}`: accept, reject, edit, and override input keyed by candidate id;
- `review.temporal-review.{json,ron}`: complete review pinned to the exact proposal and profile;
- `receipt.temporal-receipt.{json,ron}`: independently replayable input packs, proposal, review, hashes, and output;
- `enriched.character.{json,ron}`: exact accepted output profile; and
- `runtime/`: a separate locked domain project and byte-stable Story IR exposing only the read-only reviewed projection.

## Reproduce offline

From the repository root:

```bash
cargo run -p weave-character --example character_fixture -- --check

cargo run -p weave-character -- context-propose \
  examples/domain-modules/weave-character/context/input.character.json \
  --pack examples/domain-modules/weave-character/context/apollo_11.temporal-pack.json \
  --pack examples/domain-modules/weave-character/context/calendar.temporal-pack.json \
  --pack examples/domain-modules/weave-character/context/world.temporal-pack.json \
  --config examples/domain-modules/weave-character/context/ranking.temporal-config.json \
  --seed 19690720 \
  --output target/proposal.temporal-proposal.json

cargo run -p weave-character -- context-review \
  target/proposal.temporal-proposal.json \
  examples/domain-modules/weave-character/context/decisions.temporal-review.json \
  --reviewer org.weave.reviewer.fixture \
  --rationale "Review every ranked temporal cue, preserve fact and fictional-cue lineage separately, and keep all accepted material outside canon." \
  --output target/review.temporal-review.json

cargo run -p weave-character -- context-apply \
  examples/domain-modules/weave-character/context/input.character.json \
  --pack examples/domain-modules/weave-character/context/apollo_11.temporal-pack.json \
  --pack examples/domain-modules/weave-character/context/calendar.temporal-pack.json \
  --pack examples/domain-modules/weave-character/context/world.temporal-pack.json \
  target/proposal.temporal-proposal.json \
  target/review.temporal-review.json \
  --output target/receipt.temporal-receipt.json

cargo run -p weave-compiler -- \
  examples/domain-modules/weave-character/context/runtime/ari-vale-temporal.weave \
  --locked \
  --format json \
  --output target/ari-vale-temporal.story.json
```

With the exact rationale above, the proposal, review, receipt, and locked Story IR reproduce their checked JSON bytes. Use `--format ron` and the RON input pairs for the checked RON artifacts. `context-apply --dry-run` reproduces and validates the receipt without writing output.
