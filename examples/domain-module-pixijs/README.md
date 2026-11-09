# Domain module in PixiJS

This PixiJS v8 example imports the canonical compiled JSON tracer, Weave World reference seed, and
complete synthetic Weave Character profile plus its reviewed temporal-context story. It decodes
their tagged domain values and renders them with `Application` and `Text`. It reads the same trait
variables as the Bevy example, labels OCEAN as derived and lossy, accepts only reviewed/edited/
overridden alignment labels with exact pack and review fingerprints, excludes rejected and
withheld axes, and validates attributed pronouns, palette colors, safe asset paths, catalog
coordinates, assignment value kinds, and presentation locks. Its projection reader validates
approved categorical, vocation, social-role, and narrative-role labels with exact pack/proposal/
review hashes, explanations, rationales, input paths, lossiness, decisions, locks, and the all-false
write-back contract. Its expression reader validates and
renders a normalized term, categorized preference, reusable voice instruction, exact dialogue
template/scenario, and pinned pack coordinate. It keeps fact lineage separate from original
fictional-cue lineage, verifies expression, presentation, alignment, and temporal personality
write-back are false, and has no dependency on the Weave Editor.

The scene also consumes the compact Character runtime pack from
`examples/character-pack-bevy/fixtures/runtime-pack.character-pack.json`. The pure
`character-pack.js` reader validates the pack version and references, then selects the same display
name, accent colour, greeting line, personality band, alignment label, birth date, inner-life,
voice, and date-context values that the Bevy consumer reports. `character-pack.test.js` covers
successful selection and rejection of an unsupported version, a missing relationship reference, and
a noncanonical colour.

The scene also consumes the checked Plug-And-Play, Dungeonpunk, and Freehack domain packs. Pure
readers validate exact adapter coordinates and public consequences. Freehack uses a dedicated
public-only story/receipt schema and rejects authority event kinds, entropy, request hashes, hidden
opposition, secret tracks/memories, and private submissions; no authority artifact is imported by
the PixiJS bundle.

## Tabletop adapter replay consumer

`src/tabletop-adapters.js` and `src/tabletop-adapter-fixtures.node.js` load and validate every
adapter's checked-in replay data, then render the public events, final state hash, and observable
values with PixiJS `Text`. Coverage is:

| Adapter | Source | Steps | Public events |
|---|---|---|---|
| Plug-And-Play | fixture tree | 7 | 7 |
| Dungeonpunk | fixture tree | 11 | 20 |
| Freehack | public receipts and public state | 14 | 15 |
| Open Tale | fixture tree | 11 | 28 |
| Spark | canonical project | 6 | 8 |
| Wicked Ones | canonical project | 1 | 1 |
| Open Adventure | canonical project | 2 | 2 |

`scripts/tabletop-public-fixtures.mjs` writes `src/tabletop-fixtures.generated.json`. It reads the
authority-capable fixture files with Node built-ins, validates the complete state-hash chain, and
keeps only public events and public state, so the browser bundle never contains an authority
payload. `npm test` and `npm run build` regenerate the projection first.

```bash
cd examples/domain-module-pixijs
npm ci
npm test
npm run build
```

The tests cover every adapter, reject an unsupported state version, a mismatched adapter
coordinate, a broken final-state chain, a Freehack receipt carrying an authority field, and a stale
canonical project chain. All checks run offline against the checked-in synthetic fixtures; no
upstream art or prose is bundled.

Run `npm run dev` for the browser view. The pure value-reader tests intentionally run in Node so
local checks can verify the portable boundary without a GPU or browser.
