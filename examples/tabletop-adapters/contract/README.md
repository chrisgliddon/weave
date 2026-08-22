# Tabletop adapter contract fixture

Lantern Trail is a deliberately tiny, wholly original MIT-licensed fixture for the public
`weave-tabletop` contract. It is not derived from, compatible with, or presented as any published
game. Its only purpose is to exercise generic adapter discovery, character creation, isolated
mutable state, deterministic entropy, typed events, visibility, save/restore, and replay.

The exact independently authored procedure is pinned in `source/synthetic-rules.txt`; `LICENSE`
contains the redistributed license text. The adapter manifest records both SHA-256 values, its
authored revision, retrieval date, covered material, required exclusions, attribution, notices,
and a non-endorsement compatibility statement. No logo, trade dress, artwork, layout, community
supplement, or unverified asset is present.

## Checked artifacts

- `synthetic.tabletop-adapter.{json,ron}` declares schema version 1, generic creation panels,
  four capabilities, one typed resolver operation, three typed event kinds, the conservative host
  visibility matrix, and exact provenance.
- `selection.tabletop-selection.{json,ron}` installs and selects the exact adapter id, semantic
  version, state-schema version, and manifest SHA-256.
- `character.tabletop-projection.{json,ron}` references canonical Character data only by id and
  hash, keeps the adapter definition namespaced, forbids canonical write-back, and retains one
  unapplied explainable suggestion.
- `state.tabletop-state.{json,ron}` stores only adapter-owned mutable state, the exact immutable
  definition hash, and the deterministic SHA-256-counter seed/cursor.
- `request.tabletop-request.{json,ron}` and `receipt.tabletop-receipt.{json,ron}` demonstrate an
  exact replayable transition with public, authoring-only, and host-only events.
- `runtime.tabletop-receipt.json` retains all event envelopes and payload fingerprints while
  redacting the authoring and host-only payloads.
- `runtime/` projects the exact adapter into an ordinary declarative domain manifest/pack and
  compiles a `.weave` story that reads only declared capability, definition, state, and coordinate
  paths. An undeclared capability is therefore an ordinary static unknown-path diagnostic.
- `invalid/` covers conflicting primary adapters, an unsupported manifest version, and an
  operation whose capability was never declared.

Every JSON/RON pair decodes to the same Rust value and is generated from one source object. The
fixture generator also independently replays the resolver receipt before writing anything.

## Reproduce offline

```bash
cargo run -p weave-tabletop --example tabletop_fixture -- --check

cargo run -p weave-tabletop -- validate manifest \
  examples/tabletop-adapters/contract/synthetic.tabletop-adapter.json

cargo run -p weave-tabletop -- license-gate \
  examples/tabletop-adapters/contract/synthetic.tabletop-adapter.json \
  --source-artifact examples/tabletop-adapters/contract/source/synthetic-rules.txt \
  --license-text examples/tabletop-adapters/contract/LICENSE

cargo run -p weave-tabletop -- validate request \
  examples/tabletop-adapters/contract/request.tabletop-request.json \
  --manifest examples/tabletop-adapters/contract/synthetic.tabletop-adapter.json \
  --state examples/tabletop-adapters/contract/state.tabletop-state.json
```

See the [tabletop adapter guide](../../../docs/tabletop_adapters.md) for the full lifecycle,
extension-surface decision, diagnostic policy, visibility model, licensing gate, and contributor
boundary.
