# Character pack in Bevy

This offline Bevy 0.18 example loads two checked-in portable Weave Character packs:

- `fixtures/authoring-pack.character-pack.json` — the full authoring export.
- `fixtures/runtime-pack.character-pack.json` — the compact runtime export.

The example validates both with `weave-character`, independently re-derives the runtime export from
the authoring pack, and requires it to equal the checked-in runtime fixture. It then reads approved
identity, presentation, personality, alignment, birth, inner-life, voice, and date-context values
and reports the selected display name, accent colour, and greeting line from an explicit Bevy
`Startup` schedule. It has no editor dependency, performs no network access, and discovers no
artifacts at runtime.

The example also proves the failure boundary: an unsupported `pack_format_version` fails with
`unsupported character pack format version`, and an override reference absent from the pack fails
with `override id is absent from the pack`.

```bash
cargo run -p weave-example-character-pack-bevy
cargo test -p weave-example-character-pack-bevy
```

The process performs one Bevy update and exits, so the same command is suitable for documentation
and local smoke checks. Regenerate both fixtures with:

```bash
cargo run -p weave-example-character-pack-bevy --example generate_fixtures
```

Fixtures and all sample values are original synthetic Weave material licensed MIT like the rest of
the repository. See `fixtures/README.md`.
