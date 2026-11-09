# Open Tale adapter fixture

This directory contains Weave's independently authored compatibility adapter for the public
Open Tale rules reference at <https://open-tale.github.io/>. Open Tale is released under the
Apache License 2.0. This project is not sponsored or endorsed by the Open Tale project.

`SOURCE.lock.json` pins the canonical URL, the implemented edition, the authored source record,
the source-record SHA-256, the Apache-2.0 declaration, and the retained license-text digest.
`LICENSE-APACHE-2.0.txt` retains the complete Apache-2.0 text. No upstream prose, artwork, fonts,
or layout is redistributed. All fixture names and prose are synthetic.

## End-to-end fixture

- `creation.tabletop-creation.{json,ron}` is the authored creation request: seed 42, nine
  attributes allocating 18 points, active and passive skills, items, wealth, and one heroic badge.
- `preview.tabletop-creation-preview.{json,ron}` records the derived qualities, wealth spend,
  request hash, immutable definition, and initial state.
- `initial-state.tabletop-state.{json,ron}` is the resumable starting save.
- `playthrough/` runs two challenges, initiative, movement, three combat turns, recovery,
  advancement, a purchase, and a final inspection. Each step has an exact request and receipt in
  JSON and RON.
- `final-state.tabletop-state.{json,ron}` is the resumable final save.
- `playthrough/index.json` lists the ordered steps and the event kinds each receipt carries.

The playthrough is deterministic: seed 42 and the recorded choices reproduce every receipt,
entropy cursor, and state hash.

## Consumers

Standalone Rust is the authoritative consumer. It decodes every JSON and RON artifact, checks that
the pairs are equal, re-executes every request from the previous state, replays every receipt, and
compares the final state:

```bash
cargo test -p weave-tabletop --test open_tale
```

PixiJS boundary: `consumers/pixijs/consumer.mjs` reads the same JSON fixtures with only Node
built-ins, checks the receipt and event sequence, and prints the portable event list. It exports
`renderEvents` for a host that supplies a PixiJS v8 `Application`. The workspace does not install
PixiJS in this fixture tree:

```bash
node examples/tabletop-adapters/open-tale/consumers/pixijs/consumer.mjs
```

Bevy boundary: the consumer is now a workspace member. Replay all eleven steps with:

```bash
cargo run -p weave-example-bevy-tabletop --bin open-tale
```

The old reference file moved into that crate; `consumers/bevy/README.md` points at the command.

## Regenerate

Regeneration is explicit and deterministic:

```bash
cargo test -p weave-tabletop --test open_tale -- --ignored --exact generate_open_tale_fixtures
```

The generator writes the fixtures from the adapter, so a changed manifest or rule requires a
reviewed regeneration.
