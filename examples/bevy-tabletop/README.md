# Bevy tabletop consumer

A headless Bevy example that loads every checked-in tabletop adapter fixture tree, replays each
recorded playthrough with the adapter's trusted resolver, and compares every generated receipt and
the final state with the checked-in JSON. It is a host consumer only: all rules live in
`weave-tabletop`, and no editor or GPUI code is involved.

## Commands

Run every adapter:

```bash
cargo run -p weave-example-bevy-tabletop
```

Run one adapter:

```bash
cargo run -p weave-example-bevy-tabletop -- plug-and-play
cargo run -p weave-example-bevy-tabletop -- dungeonpunk
cargo run -p weave-example-bevy-tabletop -- freehack
cargo run -p weave-example-bevy-tabletop -- open-tale
cargo run -p weave-example-bevy-tabletop -- spark
cargo run -p weave-example-bevy-tabletop -- wicked-ones
cargo run -p weave-example-bevy-tabletop -- open-adventure
```

Run Open Tale's eleven-step reference playthrough through its dedicated binary:

```bash
cargo run -p weave-example-bevy-tabletop --bin open-tale
```

Run the fixture tests without a window:

```bash
cargo test -p weave-example-bevy-tabletop
```

The process exits non-zero if a creation preview, receipt, event sequence, or final state stops
matching its checked-in artifact.

## Coverage

Every shipped adapter has checked-in replay data. Four use complete fixture trees. Three use the
canonical synthetic project format from the adapter catalog.

| Adapter | Replay source | Steps |
|---|---|---|
| Plug-And-Play | `examples/tabletop-adapters/plug-and-play` fixture tree | 7 |
| Dungeonpunk | `examples/tabletop-adapters/dungeonpunk` fixture tree | 11 |
| Freehack | `examples/tabletop-adapters/freehack` fixture tree (authority host) | 16 |
| Open Tale | `examples/tabletop-adapters/open-tale` fixture tree | 11 |
| Spark | `examples/tabletop-adapters/spark/project` canonical project | 6 |
| Wicked Ones | `examples/tabletop-adapters/wicked-ones/project` canonical project | 1 |
| Open Adventure | `examples/tabletop-adapters/open-adventure/project` canonical project | 2 |

The four fixture trees provide a checked-in creation request, creation preview, starting state,
playthrough requests and receipts, and final state. Freehack is replayed as an authority host: the
consumer reads the checked-in `*.authority-receipt.json` artifacts, which retain every private
payload, and it never prints those payloads. The observable output is the adapter state, not
host-only event data.

Spark, Wicked Ones, and Open Adventure provide a canonical synthetic project instead: a seed, a
scripted operation path, and expected creation, request, receipt, state, and event hashes. This
consumer creates deterministically through the public conformance adapter, rebuilds each request,
and compares every hash and event kind with `expected-output.json`.

## How replay works

Fixture trees:

1. Parse the checked-in manifest and require it to equal the live Rust manifest.
2. Parse the creation request, regenerate its preview, and require the checked-in preview and
   initial state to match.
3. Read every request and receipt pair. Order steps by the recorded state-hash chain rather than by
   filename, so generation order is recovered exactly.
4. Execute each request from the preceding state, compare the generated receipt byte-for-byte with
   the checked-in receipt, and run the explicit replay check.
5. Require the final replayed state to equal the checked-in final state and, for Open Tale, compare
   the order and event kinds with `playthrough/index.json`.

Canonical projects:

1. Parse `project.json` and `expected-output.json`, and require the exact adapter coordinate, seed,
   synthetic MIT source, and project format version.
2. Create deterministically from the recorded seed and compare the manifest, creation preview,
   definition, and initial-state hashes.
3. Require the checked-in scripted path to equal the live conformance scenario.
4. Execute each operation from the preceding state and compare the request hash, receipt hash,
   before and after state hashes, revision, entropy consumption, and event kinds.
5. Require the final state hash to match.

Observable values are the sorted scalar leaves of the final adapter `DomainValue`. The same
flattening rule is implemented by the PixiJS consumer, so both render identical paths and values.

## Fixture provenance

The fixture trees are synthetic and checked in with the adapters. Plug-And-Play, Dungeonpunk, and
Freehack retain their CC0-1.0 license text and reviewed source locks; Open Tale retains
Apache-2.0. This example bundles no upstream artwork, prose, or network dependency.
