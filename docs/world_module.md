# Weave World reference seeds

Weave World is an optional data-only domain module. Four checked presets turn exact reference-place selectors into closed `WorldSeed` values. A deterministic composition plan can combine selected fields from broad, regional, and ecosystem packs into another ordinary pack, after which source-authored rules and stable fictional places remain authoritative. The compiler, editor, runtime, RON, JSON, Bevy, and PixiJS all read the result through shared portable contracts.

The checked fixture is [`examples/domain-modules/weave-world`](https://github.com/chrisgliddon/weave/tree/main/examples/domain-modules/weave-world). It is an environmental seed for fictional authoring, not a cultural profile. The exported identity explicitly records `culture_included: false`; the preset does not infer people, language, naming, or behavior from geography.

## Select a reference place

The pack coordinate is the concise, versioned shorthand:

```weave
module world {
    id: "org.weave.world"
    version: "=1.1.0"
    pack: "aotearoa_new_zealand@=1.1.0"
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

The corpus deliberately covers different environments and geographic scales:

| Preset coordinate | Scope | Climate cue | Primary biome |
|---|---|---|---|
| `aotearoa_new_zealand@=1.1.0` | Country, selected-station envelope | Temperate oceanic | Temperate broadleaf and mixed forest |
| `hokkaido_japan@=1.1.0` | Region, representative grid point | Humid continental | Temperate broadleaf and mixed forest |
| `maldives@=1.1.0` | Country, representative grid point | Tropical oceanic | Tropical and subtropical moist broadleaf forest |
| `british_columbia_temperate_forest@=1.1.0` | Ecosystem, representative grid point | Temperate oceanic | Temperate conifer forest |

Each preset has its own runnable `.weave` source and checked RON/JSON pair. A story activates one pack at a time because mutable domain state is isolated by module identity.

## Typed seed

`world.seed` exposes statically discoverable fields for:

- a compact attribution notice retained in portable runtime exports;
- identity and stable preset coordinate;
- climate band, `1991-2020` reference period, explicit Celsius and annual-millimetre units, observation ranges, source resolution, confidence, and uncertainty;
- geographic resolution and the exact selected-station or representative-grid-point approximation policy;
- representative biome classes and one primary biome for concise conditions;
- conservative terrain and water context;
- hemisphere-aware warmest and coolest months;
- idealized solstice daylight calculated from a declared representative latitude; and
- evidence-backed weather cues plus deterministic environmental hazard tendencies.

Every reference pack is deliberately approximate. A selected-station envelope or source-native grid point is not a local forecast, and broad symbols are authoring cues rather than exhaustive geographic claims. Hazard entries are tendencies derived by a closed threshold policy, never event forecasts, probabilities, or risk scores. Later fictional places and overrides should remain separate authored layers rather than silently changing a pinned source seed.

## Compose reviewed layers

[`composition.weave-world.json`](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/weave-world/composition.weave-world.json) is a versioned, closed plan. The checked Glasswind build applies these scopes in order:

| Layer | Selected pack | Included paths | Conflict policy |
|---|---|---|---|
| Broad reference | `aotearoa_new_zealand@1.1.0` | Complete `seed` | Reject overlap |
| Regional climate | `hokkaido_japan@1.1.0` | Climate, daylight, seasonality | Replace broad leaves |
| Ecosystem surface | `british_columbia_temperate_forest@1.1.0` | Hazards, primary biome, terrain, water, weather | Replace earlier leaves |

The composer validates every candidate against the same World manifest, flattens only the selected paths to leaves, applies layers in canonical `broad` → `regional` → `ecosystem` order, then reconstructs and validates a normal `DomainPack`. Within a scope, layer ids are sorted. Included paths must be sorted, unique, present in every candidate, and non-overlapping within that layer. Every overlap declares one policy:

- `reject` stops composition at the first existing leaf;
- `keep` preserves the earlier authoritative value and records the ignored incoming layer; and
- `replace` makes the later layer authoritative, recording `confirmed` for an equal value or `replaced` for a different value.

`exact` selection permits one candidate. `seeded` selection permits a sorted candidate set and hashes the format domain separator, public `random_seed`, zero-based layer index, and layer id with SHA-256; the first eight digest bytes, interpreted little-endian and reduced by candidate count, select the index. The receipt retains that entire candidate set, selected index, selected coordinate, include paths, policy, and every leaf decision, so another implementation can reproduce the choice rather than trusting an opaque result.

Compose the checked pack and JSON receipt without network access:

```bash
cargo run -p weave-world -- compose \
  examples/domain-modules/weave-world/composition.weave-world.json \
  --manifest examples/domain-modules/weave-world/module.weave-module.json \
  --pack examples/domain-modules/weave-world/pack.weave-domain.json \
  --pack examples/domain-modules/weave-world/packs/hokkaido_japan.weave-domain.json \
  --pack examples/domain-modules/weave-world/packs/british_columbia_temperate_forest.weave-domain.json \
  --output target/glasswind_composed.weave-domain.json \
  --receipt target/glasswind-composition.receipt.json
```

The output pack declares exact dependencies on the three selected inputs. Existing domain resolution therefore pins the composed pack and its complete input closure in `weave.lock`; unselected seeded candidates are reviewable in the plan and receipt but do not enter the runtime dependency graph. The generated pack copies and namespaces the selected public provenance without changing any input bytes. [`composed-setting.weave`](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/weave-world/composed-setting.weave) then selects `glasswind_composed@=1.0.0`; its ordinary `override` entries apply after all pack composition and give the setting a fictional identity, rules, places, and local exceptions.

The [composition-plan schema](downloads/weave-world-composition-v1.schema.json) is format version 1. The checked output pack, JSON/RON receipt, `.weave` source, and Story IR JSON/RON pair live beside the plan.

## Author rules and stable places

[`authored-setting.weave`](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/weave-world/authored-setting.weave) is the complete fictional layer. It uses the generic module replacement syntax; the compiler contains no branch for the `org.weave.world` identity:

```weave
module world {
    id: "org.weave.world"
    version: "=1.1.0"
    pack: "aotearoa_new_zealand@=1.1.0"

    override rules.booleans.beacons_answer_storms: true
    override rules.numbers.safe_crossing_temperature_c: 6

    override places.glasswind_reach.id: "glasswind_reach"
    override places.glasswind_reach.name: "Glasswind Reach"
    override places.glasswind_reach.kind: region
    override places.glasswind_reach.order: 0
    override places.glasswind_reach.parent_id: ""
    override places.glasswind_reach.related_place_ids: []
    override places.glasswind_reach.environment_source: ""
    override places.glasswind_reach.climate_source: ""

    override places.emberwake_harbor.id: "emberwake_harbor"
    override places.emberwake_harbor.name: "Emberwake Harbor"
    override places.emberwake_harbor.kind: settlement
    override places.emberwake_harbor.order: 0
    override places.emberwake_harbor.parent_id: "glasswind_reach"
    override places.emberwake_harbor.related_place_ids: []
    override places.emberwake_harbor.environment_source: "glasswind_reach"
    override places.emberwake_harbor.climate_source: "glasswind_reach"
    override places.emberwake_harbor.environment_override.coastal: true
}
```

`rules` is an extensible object split into Boolean, number, string, and symbolic-string maps, so a narrative reads a statically typed path such as `world.rules.booleans.beacons_answer_storms`. `places` is a bounded map keyed by stable lowercase ids. Changing `name` is a rename; it does not change the id used by `parent_id`, `related_place_ids`, environment/climate inheritance, narrative paths, or consumers. `order` controls deterministic presentation independently of JSON object order.

Every place explicitly chooses `environment_source` and `climate_source`: the empty string means the immutable reference seed, while another stable place id inherits that place's effective values. Optional `environment_override`, `climate_override`, and type-separated `attributes` objects record local fictional decisions. Validation rejects mismatched map/id pairs, unknown or self references, duplicate/asymmetric relationships, and cycles in hierarchy or either inheritance graph.

The editor's generic entity workflow reads `authoring.entity_collections` from the manifest. Create, rename, reorder, nest, relate, select inheritance, and reset-override actions rewrite canonical source and recompile atomically. `ModuleInspection` shows the effective hierarchy and marks pack values as inherited or generated and source replacements as authored. When given the selected composition receipt it also previews the ordered layers and leaf differences. “Promote to authored” copies one effective generated leaf into canonical source, recompiles atomically, and can be reset to reveal the composed value again. The host-independent `weave-world` crate additionally resolves each local climate/environment field and retains its recursive lineage—for example generated from `seed.primary_biome`, inherited through `glasswind_reach`, or authored at `places.emberwake_harbor.environment_override.coastal`.

Compiled IR embeds effective `rules` and `places` for ordinary consumers and keeps the sorted fictional leaves in `authored_overrides`. The selected pack bytes and their public provenance remain unchanged. Checked RON and JSON are [`authored-setting.story.ron`](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/weave-world/authored-setting.story.ron) and [`authored-setting.story.json`](https://github.com/chrisgliddon/weave/blob/main/examples/domain-modules/weave-world/authored-setting.story.json).

## Full and compact portable World exports

Portable World export format 1 has two projections in both stable JSON and RON:

| Projection | Retained | Deliberately omitted |
|---|---|---|
| Full | Composition receipt, authored override paths, resolved rules/places/environment/climate, and recursive generated/authored/inherited origins | Nothing from the authoring lineage boundary |
| Compact | Exact module/pack coordinate plus resolved rules, roots, places, environment, and climate | Composition, candidate choices, differences, authored paths, origins, and source paths |

The compact form is not a lossy resolver input: it is the final runtime projection after inheritance. The full form is the inspection/audit projection. Both reject unknown fields, unsupported versions, malformed hierarchy/relationship graphs, or inconsistent pack metadata without echoing values. See the checked [full schema](downloads/weave-world-full-v1.schema.json), [compact schema](downloads/weave-world-compact-v1.schema.json), and `composed-world.{full,compact}.{json,ron}` fixtures.

## Optional names stay separate

Environmental shorthand never proposes names. The default authored story activates only `org.weave.world`, retains `culture_included: false`, and uses names explicitly written by its author. A separately packaged optional `org.weave.world.naming` module demonstrates the extension boundary with four original fictional suggestions under `naming/`. Its manifest and pack are independently licensed `CC0-1.0`, say that no geographic or cultural inference was used, and are not activated by default. Projects with reviewed naming data can activate such a module explicitly; a geographic preset can never pull it in implicitly.

## Provenance and normalization

The corpus uses [Natural Earth v5.1.2 physical vectors](https://github.com/nvkelso/natural-earth-vector/releases/tag/v5.1.2), whose vector data are [public domain](https://www.naturalearthdata.com/about/terms-of-use/); Earth Sciences New Zealand / NIWA `1991-2020` temperature and rainfall workbooks, whose downloads are [CC BY 4.0](https://niwa.co.nz/climate-and-weather/climate-data-and-activities#reuse); [RESOLVE Ecoregions 2017](https://developers.google.com/earth-engine/datasets/catalog/RESOLVE_ECOREGIONS_2017), also CC BY 4.0; and stable CSV responses from NASA POWER `1991-2020` climatologies derived from MERRA-2.

NASA describes POWER as free, globally available analysis-ready climate data. Its [science-data license policy](https://science.data.nasa.gov/about/license) covers NASA public data, while the POWER [referencing guide](https://power.larc.nasa.gov/docs/referencing/) requests the project reference, service version, access date, and notification when data is redistributed. The generated packs retain that requested attribution and classify the reviewed data under `LicenseRef-NASA-Public-Data`; this avoids implying that NASA names or logos are licensed material.

Every acquired artifact has an HTTPS URL, citation or revision and retrieval date, SHA-256, SPDX or `LicenseRef` expression, license URL, attribution, and modification flag in its generated pack. Named transformations explain filtering, unit conversion, aggregation, daylight calculation, threshold-based hazards, symbol normalization, and final seed assembly. Nested claims connect every major seed field to those transformations. A compact attribution string also remains inside `WorldSeed` when only compiled RON or JSON is redistributed. The editor's module inspection model exposes the selected values plus both manifest and pack provenance unchanged.

Composition never turns one source's license into another. Each input pack remains independently authored and redistributable under its recorded terms; the output copies those source records and transformation claims under layer-local ids, adds the MIT-licensed original composition plan, and records exact input coordinates. The checked plan combines environmental fields only. It neither derives cultural material nor activates the separate naming module.

## Offline corpus pipeline

The checked [preset schema](downloads/weave-world-corpus-preset-v1.schema.json) and [index schema](downloads/weave-world-corpus-index-v1.schema.json) define a closed contributor format. `corpus.weave-world.json` maps local source records to generated pack paths. `weave-world-corpus` reads only those local files and the local module manifest; the builder has no acquisition or network step.

Rebuild and then prove the outputs are byte-exact:

```bash
cargo run -p weave-world-corpus -- build \
  examples/domain-modules/weave-world/corpus.weave-world.json \
  --manifest examples/domain-modules/weave-world/module.weave-module.json

cargo run -p weave-world-corpus -- check \
  examples/domain-modules/weave-world/corpus.weave-world.json \
  --manifest examples/domain-modules/weave-world/module.weave-module.json
```

Corpus v1 applies these deterministic policies:

| Input situation | Closed policy |
|---|---|
| Missing required fact, units, resolution, uncertainty, source role, or attribution | Reject the source record before generation. |
| Multiple source observations disagree | Combine only observations declared under the same reference period, units, source resolution, and geographic approximation; preserve their extrema as a range and deterministically average monthly values for season ranking. Reject incompatible scope, period, unit, or record shape instead of silently choosing a source. |
| Primary biome absent from the declared biome set, island system without coast, inverted range, or warm/cool month contradiction | Reject the contradictory record. |
| Temperature or precipitation unit outside the closed vocabulary | Reject it; v1 accepts Celsius plus either source daily millimetres or already annual millimetres. |
| Daily precipitation rate | Multiply by the exact mean Gregorian days per year in the inclusive reference period, then round to one decimal annual millimetre. |
| Representative grid point used for a broad country, region, or ecosystem | Require `representative_not_exhaustive`, an explicit scale, source resolution, approximation tag, and uncertainty statement. |
| Daylight | Use WGS84 representative latitude, 23.44° solstice declination, polar clamping, and one-decimal hours; exclude refraction, terrain, and twilight. |
| Hazards | Apply only the documented v1 precipitation, freezing, wet-slope, coastal-atoll, and curated-cyclone rules. Missing evidence produces no inferred tendency. |
| Public source without exact hash or a reviewed open license | Reject it. |

To add a preset without changing module, compiler, runtime, or editor code:

1. Review redistribution terms for every source and acquire the exact bytes outside the runtime pipeline.
2. Add one `corpus/presets/*.json` record with facts, units, scale, uncertainty, exact URL/revision/hash/license/attribution, and source roles.
3. Add its sorted source/output entry to `corpus.weave-world.json`.
4. Run `weave-world-corpus build`, then `check` and `weave-module validate`.
5. Add one single-pack `.weave` story plus exact RON/JSON artifacts and run the workspace tests.

No cultural, linguistic, population, naming, or behavioral facts belong in these environmental records.

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
  --module-manifest examples/domain-modules/weave-world/module.weave-module.json \
  --module-pack examples/domain-modules/weave-world/pack.weave-domain.json \
  --output target/reference-place.story.ron

cargo run -p weave-compiler -- \
  examples/domain-modules/weave-world/composed-setting.weave \
  --locked --format json --output target/composed-setting.story.json
```

Compile the single-pack authored layer with explicit `module.weave-module.json` and `pack.weave-domain.json`, as shown for the reference story. The checked project lock intentionally describes the composed build and its three-pack dependency closure.

Both forms decode to the same `StoryIr` and embed the selected seed plus any effective authored rules and places, so hosts need neither the editor nor the source datasets. The finite [Bevy consumer](https://github.com/chrisgliddon/weave/tree/main/examples/domain-module-bevy) reads the RON into resources and derives beacon-escort behavior plus a `cedar-snow` presentation token. The [PixiJS v8 consumer](https://github.com/chrisgliddon/weave/tree/main/examples/domain-module-pixijs) decodes the JSON with the same portable value reader and changes its actual palette and travel copy. Both decisions depend on the composed climate/biome together with authored rule, place, and local environment values.

The three additional reference story pairs live under `examples/domain-modules/weave-world/corpus/stories`; the authored and composed setting pairs live beside the reference story. Keep cultural or linguistic material in a separately sourced, explicitly reviewed module; environmental reference shorthand and composition must never synthesize it.
