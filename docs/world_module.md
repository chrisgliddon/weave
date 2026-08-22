# Weave World reference seeds

Weave World is an optional data-only domain module. Its first complete preset turns one exact reference-place selector into a closed `WorldSeed` that the compiler, editor, runtime, RON, JSON, Bevy, and PixiJS all read through the shared domain-module contract.

The checked fixture is [`examples/domain-modules/weave-world`](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/weave-world). It is an environmental seed for fictional authoring, not a cultural profile. The exported identity explicitly records `culture_included: false`; the preset does not infer people, language, naming, or behavior from geography.

## Select a reference place

The pack coordinate is the concise, versioned shorthand:

```weave
module world {
    id: "org.weave.world"
    version: "=1.0.0"
    pack: "aotearoa_new_zealand@=1.0.0"
}

VAR reference_name = world.seed.identity.display_name
VAR reference_climate = world.seed.climate.band

=== start ===
{reference_climate == temperate_oceanic:
    {reference_name} seeds a {world.seed.primary_biome} world with an {world.seed.water.setting} setting.
- else:
    The reference-place preset is unavailable.
}
-> END
```

The exact selector keeps resolution deterministic. The adjacent `weave.modules.json` bounds discovery to reviewed project files, while `weave.lock` pins the canonical manifest and pack hashes. Compilation with `--locked` neither searches user directories nor contacts the network.

## Typed seed

`world.seed` exposes statically discoverable fields for:

- a compact attribution notice retained in portable runtime exports;
- identity and stable preset coordinate;
- climate band, `1991-2020` reference period, units in field names, station-range values, spatial resolution, and confidence;
- representative biome classes and one primary biome for concise conditions;
- country-scale coastal, island, and mountain terrain;
- coastal and oceanic-island water context;
- southern-hemisphere warmest and coolest months; and
- evidence-backed alpine cooling, rainfall contrast, and seasonal temperature tendencies.

The reference pack is deliberately approximate. Its selected-station range is not a local forecast, and broad symbols are authoring cues rather than exhaustive geographic claims. Later fictional places and overrides should remain separate authored layers rather than silently changing this pinned source seed.

## Provenance and normalization

The pack uses [Natural Earth v5.1.2 physical vectors](https://github.com/nvkelso/natural-earth-vector/releases/tag/v5.1.2), whose vector data are [public domain](https://www.naturalearthdata.com/about/terms-of-use/); Earth Sciences New Zealand / NIWA `1991-2020` temperature and rainfall workbooks, whose downloads are [CC BY 4.0](https://niwa.co.nz/climate-and-weather/climate-data-and-activities#reuse); and [RESOLVE Ecoregions 2017](https://developers.google.com/earth-engine/datasets/catalog/RESOLVE_ECOREGIONS_2017), also CC BY 4.0.

Every acquired artifact has an HTTPS URL, revision, SHA-256, SPDX or `LicenseRef` expression, license URL, attribution, and modification flag in `pack.weave-domain.json`. Named transformations explain filtering, aggregation, symbol normalization, and final seed assembly. Nested claims connect climate, biomes, identity, terrain, water, seasonality, and weather tendencies to those transformations. A compact attribution string also remains inside `WorldSeed` when only compiled RON or JSON is redistributed. The editor's module inspection model exposes the selected values plus both manifest and pack provenance unchanged.

## Diagnostics

Preset failures remain generic domain-contract failures so Weave core does not depend on a World schema:

| Situation | Diagnostic | Recovery |
|---|---|---|
| Unknown preset ID | `D151` | Install the selected pack or correct the ID before `@`. |
| Known preset, unavailable release | `D104` | Install a matching release or update the requirement after `@`. |
| Preset incompatible with the module release | `D104` | Select a pack whose module range includes the activated release. |
| Duplicate module/pack/version coordinate | Ambiguous artifact rejection | Remove the duplicate so one coordinate resolves to one artifact. |

Malformed selectors and unsupported artifact formats retain the `D152` and `D100`–`D101` contract diagnostics described in [Pluggable domain modules](domain_modules.md#failure-behavior-and-diagnostics).

## Compile and consume

```bash
cargo run -p weave-compiler -- \
  examples/domain-modules/weave-world/reference-place.weave \
  --locked --output target/reference-place.story.ron

cargo run -p weave-compiler -- \
  examples/domain-modules/weave-world/reference-place.weave \
  --locked --format json --output target/reference-place.story.json
```

Both files decode to the same `StoryIr` and embed the selected values, so hosts need neither the editor nor the source datasets. The finite [Bevy consumer](https://github.com/chrisgliddon/weave/tree/main/examples/domain-module-bevy) reads the RON into resources; the [PixiJS v8 consumer](https://github.com/chrisgliddon/weave/tree/main/examples/domain-module-pixijs) decodes the JSON with the same portable value reader.

To author another preset, add a validated pack and project activation without editing the parser, compiler, runtime, or editor schema. Keep cultural or linguistic material in a separately sourced, explicitly reviewed pack; environmental reference shorthand must never synthesize it.
