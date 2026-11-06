# Portable Character pack fixtures

These fixtures are original synthetic Weave material, authored for this repository and
licensed MIT like the rest of the project. They contain no external art, copied prose, or
third-party data.

- `authoring-pack.character-pack.json` is the full authoring export. It retains the
  effective profile, attributed provenance, reviewed presentation, alignment, and date
  context values.
- `runtime-pack.character-pack.json` is the compact runtime export generated from the
  authoring pack. It keeps only approved values and the references required to render the
  character, and it is the exact file consumed by both the Bevy and PixiJS examples.

Do not hand-edit either file. Regenerate both from the repository root:

```bash
cargo run -p weave-example-character-pack-bevy --example generate_fixtures
```

The generator is deterministic: repeated runs produce byte-identical files.
