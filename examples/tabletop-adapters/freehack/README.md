# Freehack adapter fixture

This directory contains Weave's independently authored compatibility adapter for the official
Freehack 2.1 release by Amini Allight. The reviewed release and source are CC0-1.0. This project is
independent and is not sponsored or endorsed by the creator.

`SOURCE.lock.json` pins the official itch release, exact GitLab commit, release date, retrieval
date, and SHA-256 values for the PDF, Markdown source, publication metadata, executable roll
reference, and retained CC0 legal code. The upstream PDF, prose, artwork, icon, stylesheet,
branding, layout, trade dress, community material, and unverified assets are not redistributed.

## End-to-end fixture

- `creation.tabletop-creation.{json,ron}` supplies a versioned campaign schema. It demonstrates a
  rarity-based archetype offer, authored/fixed/random arbitrary modifiers, catalog and authored
  features, budgeted catalog and authored inventory, public and host-only memories, and public and
  secret generic tracks.
- `preview.tabletop-creation-preview.{json,ron}` records every deterministic offer/value, entropy
  cursor, explanation, request hash, immutable definition, and initial authority state. The editor
  requires explicit reroll seeds and acceptance before export.
- `probability*` proves exact integer support/opposition weights and a floored basis-point display.
  Resolution uniformly samples the nonzero signed result domain with rejection sampling, then
  derives signed magnitude with checked integer arithmetic.
- `playthrough/` exercises probability preview, public and host-only checks, public memory, secret
  track progress, track creation/completion, and mapped/unmapped timed simultaneous sections. It
  covers private submission, cancellation, replacement, deterministic participant order,
  resolution, timeout, save/restore, JSON/RON, and replay.
- `*-authority-*` artifacts retain complete host state and private audit payloads. `public-*`
  artifacts use dedicated schemas that omit entropy, request hashes, event visibility envelopes,
  hidden opposition, secret tracks, private memories, and unrevealed submissions. A host-only
  operation produces no public receipt at all.
- `runtime/` contains a deliberately smaller public-only module and pack. The compiled Tavi Quill
  story cannot address authority state. Bevy validates both channels as an authority host without
  logging private values; PixiJS imports only the public story and receipt.

## Reproduce locally

```bash
cargo run -p weave-tabletop --example freehack_fixture -- --check

cargo run -p weave-tabletop -- freehack-create \
  examples/tabletop-adapters/freehack/creation.tabletop-creation.json \
  --output target/freehack-preview.json

cargo run -p weave-tabletop -- freehack-probability \
  examples/tabletop-adapters/freehack/probability.freehack-probability.json \
  --output target/freehack-probability-preview.json

cargo run -p weave-tabletop -- freehack-resolve \
  examples/tabletop-adapters/freehack/playthrough/preview_crossing_probability.tabletop-request.json \
  --state examples/tabletop-adapters/freehack/authority-state.tabletop-state.json \
  --authority-output target/freehack-authority-receipt.json \
  --public-output target/freehack-public-receipt.json
```

Source verification is also local and performs no network access. First acquire the exact files
named in `SOURCE.lock.json`, preserving the nested companion paths, then run:

```bash
cargo run -p weave-tabletop -- license-gate \
  examples/tabletop-adapters/freehack/freehack.tabletop-adapter.json \
  --source-artifact freehack.pdf \
  --companion-artifact src/freehack.md \
  --companion-artifact src/freehack.yml \
  --companion-artifact tools/roll.py \
  --license-text examples/tabletop-adapters/freehack/LICENSE-CC0-1.0.txt
```

The gate fails closed on changed bytes, paths, revision metadata, license text, or provenance.
