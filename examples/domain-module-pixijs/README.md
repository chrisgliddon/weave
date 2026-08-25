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

The scene also consumes the checked Plug-And-Play and Dungeonpunk domain packs. Pure readers
validate exact adapter coordinates, portable state, public check/Struggle consequences, request
fingerprints, and host-only entropy audit envelopes whose payloads remain redacted.

```bash
cd examples/domain-module-pixijs
npm ci
npm test
npm run build
```

Run `npm run dev` for the browser view. The pure value-reader tests intentionally run in Node so
CI can verify the portable boundary without a GPU or browser.
