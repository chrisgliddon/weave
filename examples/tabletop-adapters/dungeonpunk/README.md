# Dungeonpunk adapter fixture

This directory contains Weave's independently implemented compatibility adapter for the official
eight-page Dungeonpunk core rules by Ash McAllan. The reviewed core document is released under
CC0-1.0. This project is not sponsored or endorsed by the creator.

The official Google Doc PDF and plain-text exports are audit inputs pinned by source URL, revision,
retrieval date, media type, and SHA-256 in `SOURCE.lock.json`; they are not redistributed. The CC0
1.0 legal code is retained as `LICENSE-CC0-1.0.txt`. No official prose, artwork, logo, branding,
trade dress, layout, wiki content, community supplement, or unverified asset is included.

## End-to-end fixture

- `creation.tabletop-creation.{json,ron}` allocates exactly five points across six stats, records
  FATE 1 and NONE 0, selects three declarative core moves, and provides sorted gear, two bonds, a
  clock, and a threat.
- `preview.tabletop-creation-preview.{json,ron}` exposes the deterministic 3d6-plus-Constitution HP
  result, exact half-Weight load, request and definition fingerprints, immutable definition, and
  initial mutable state.
- `character.tabletop-projection.{json,ron}` isolates the accepted adapter definition from canonical
  Character data and forbids write-back.
- `playthrough/` proves helped-and-pushed and non-positive Struggle pools, highest/lowest selection,
  failure XP and structured GM prompts, Stress-derived encumbrance, rest, damage and Brace, growth,
  an armor-bearing threat, a triggering clock, and a closed GM move. Every request and receipt is
  exact JSON/RON and replayed by the generator.
- `state.tabletop-state.{json,ron}` is the initial save, while
  `final-state.tabletop-state.{json,ron}` is the portable save after the complete playthrough.
- `runtime/` lowers the accepted definition and final state into an ordinary declarative domain
  module/pack and compiles `vesper-ash.weave` for standalone, Bevy, and PixiJS consumers.
- `runtime.tabletop-receipt.json` retains public consequences while redacting the host-only entropy
  trace payload without removing its audit envelope or hash.

The resolver models positive pools as that many d6 selecting the highest result and non-positive
pools as 2d6 selecting the lowest. Six succeeds, four or five introduces a twist, and three or less
fails. Help adds a die and records one Stress for the helper; pushing adds a die and two Stress;
failure earns XP. Stress contributes one Weight, more than twelve Weight encumbers, rest recovers
one HP and one Stress per complete two hours, and below-zero HP creates a separate deterministic
FATE death-check transition. Growth, gear damage, clocks, threats, and GM moves remain structured
portable data.

## Reproduce offline

```bash
cargo run -p weave-tabletop --example dungeonpunk_fixture -- --check

cargo run -p weave-tabletop -- dungeonpunk-create \
  examples/tabletop-adapters/dungeonpunk/creation.tabletop-creation.json \
  --output target/dungeonpunk-preview.json

cargo run -p weave-tabletop -- dungeonpunk-resolve \
  examples/tabletop-adapters/dungeonpunk/request.tabletop-request.json \
  --state examples/tabletop-adapters/dungeonpunk/state.tabletop-state.json \
  --output target/dungeonpunk-receipt.json
```

To re-run the source gate, first acquire both exact exports recorded in `SOURCE.lock.json`:

```bash
cargo run -p weave-tabletop -- license-gate \
  examples/tabletop-adapters/dungeonpunk/dungeonpunk.tabletop-adapter.json \
  --source-artifact dungeonpunk-google-doc.pdf \
  --companion-artifact dungeonpunk-google-doc.txt \
  --license-text examples/tabletop-adapters/dungeonpunk/LICENSE-CC0-1.0.txt
```

The gate performs no network access and fails closed if source bytes, companion filenames, revision
metadata, license text, or manifest provenance differ.
