# Plug-And-Play adapter fixture

This directory contains Weave’s independently implemented compatibility adapter for the public,
CC0-1.0 Plug-And-Play tabletop rules by Olav Jakobson Digranes / Distilled Productions Limited.
It is a factual compatibility implementation and is not sponsored or endorsed by the creator.

The official Rules and Character Sheet PDFs are reviewed inputs, pinned by exact filename, upload
identity, retrieval date, media type, and SHA-256 in `SOURCE.lock.json`. They are not redistributed.
The official CC0 1.0 legal code is retained verbatim as `LICENSE-CC0-1.0.txt`. No official artwork,
logo, branding, trade dress, PDF layout, community supplement, or unverified asset is included.

## End-to-end fixture

- `creation.tabletop-creation.{json,ron}` is the exact rolled creation request, including seed,
  selected candidate set, explicit d6 assignment, two independently zero-sum modifiers,
  Survivability choices, and inventory.
- `preview.tabletop-creation-preview.{json,ron}` exposes both deterministic candidate sets,
  request fingerprint, seed/cursor lineage, base and effective ratings, derived Survivability,
  immutable definition, and initial mutable state before acceptance.
- `character.tabletop-projection.{json,ron}` keeps the accepted definition isolated from canonical
  Character data and forbids write-back.
- `playthrough/` executes checks, advantage, conditional Fortune spending, a Fortune test, attack
  and damage, a group check, 54-card initiative, a chase bounded to three checks, and wound damage.
  Every request and receipt is exact JSON/RON and independently replayed by the generator.
- `state.tabletop-state.{json,ron}` is the initial save; `final-state.tabletop-state.{json,ron}` is
  the reloadable save after the complete playthrough.
- `runtime/` lowers the same accepted character and final state to an ordinary declarative domain
  module/pack and compiles `ember-vale.weave` for the standalone, Bevy, and PixiJS consumers.
- `runtime.tabletop-receipt.json` demonstrates runtime visibility projection: public event payloads
  remain readable while host-only entropy payloads retain only their audit envelope and hash.

The modeled procedures are intentionally explicit: natural 6 is a critical and natural 1 is a
fumble for ordinary checks; advantage/disadvantage selects the high/low d6; Fortune tests succeed
only below remaining Fortune and have no critical/fumble semantics; a failed ordinary check can
spend one Fortune for a mandatory replacement roll; attacks use rating plus half-rating, critical
hits deal maximum damage, melee fumbles deal half maximum damage rounded up to the attacker, and
ranged fumbles jam the weapon; zero Survivability wounds, and later positive damage kills; group
success requires more successes than failures; card initiative ranks Ace high and
Spades > Hearts > Clubs > Diamonds with valueless Jokers; chases consume at most three opposed
checks and stop on a natural critical or fumble.

## Reproduce offline

```bash
cargo run -p weave-tabletop --example plug_and_play_fixture -- --check

cargo run -p weave-tabletop -- plug-and-play-create \
  examples/tabletop-adapters/plug-and-play/creation.tabletop-creation.json \
  --output target/plug-and-play-preview.json

cargo run -p weave-tabletop -- plug-and-play-resolve \
  examples/tabletop-adapters/plug-and-play/request.tabletop-request.json \
  --state examples/tabletop-adapters/plug-and-play/state.tabletop-state.json \
  --output target/plug-and-play-receipt.json
```

To re-run the source gate, acquire both exact PDFs from the official release page and preserve
their recorded filenames:

```bash
cargo run -p weave-tabletop -- license-gate \
  examples/tabletop-adapters/plug-and-play/plug-and-play.tabletop-adapter.json \
  --source-artifact "Plug-And-Play Rules.pdf" \
  --companion-artifact "Plug-And-Play Character Sheet.pdf" \
  --license-text examples/tabletop-adapters/plug-and-play/LICENSE-CC0-1.0.txt
```

The gate performs no network access and fails closed if any bytes, filenames, companions, license
text, or manifest provenance differ.
