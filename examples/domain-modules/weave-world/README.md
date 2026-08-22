# Weave World environmental corpus

This fixture contains four complete `org.weave.world` reference paths: Aotearoa New Zealand, Hokkaido in Japan, Maldives, and the temperate forests of British Columbia. Each exact pack materializes one closed, deterministic `WorldSeed` at country, region, or ecosystem scale. All are environmental reference material for fictional authoring: `culture_included` is explicitly `false`, and no pack makes claims about people, language, identity, or culture.

Compile both portable forms from the repository root:

```bash
cargo run -p weave-compiler -- \
  examples/domain-modules/weave-world/reference-place.weave \
  --locked --output examples/domain-modules/weave-world/reference-place.story.ron

cargo run -p weave-compiler -- \
  examples/domain-modules/weave-world/reference-place.weave \
  --locked --format json \
  --output examples/domain-modules/weave-world/reference-place.story.json
```

The adjacent `weave.modules.json` is the complete preset-selection boundary. The exact lock pins the module and pack bytes; authoring and runtime do not contact the network.

## Build the corpus offline

Contributor records live in `corpus/presets`, while `corpus.weave-world.json` maps those records to the canonical packs. The Rust builder validates the closed format, explicit units and scale, open licenses, exact public-source hashes, contradictions, uncertainty, and broad-approximation policy before it uses the shared domain contract:

```bash
cargo run -p weave-world-corpus -- build \
  examples/domain-modules/weave-world/corpus.weave-world.json \
  --manifest examples/domain-modules/weave-world/module.weave-module.json

cargo run -p weave-world-corpus -- check \
  examples/domain-modules/weave-world/corpus.weave-world.json \
  --manifest examples/domain-modules/weave-world/module.weave-module.json
```

The command never downloads data. It deterministically converts reviewed daily precipitation to annual millimetres, ranks monthly temperature, estimates solstice daylight, applies the documented environmental hazard thresholds, sorts all closed sets, generates provenance transformations and claims, and validates each result against `module.weave-module.json`.

| Preset | Scale | Checked source and portable artifacts |
|---|---|---|
| `aotearoa_new_zealand` | Country, selected stations | `reference-place.weave`, `reference-place.story.ron`, `reference-place.story.json` |
| `hokkaido_japan` | Region, representative grid point | `corpus/stories/hokkaido-japan.*` |
| `maldives` | Country, representative grid point | `corpus/stories/maldives.*` |
| `british_columbia_temperate_forest` | Ecosystem, representative grid point | `corpus/stories/british-columbia-temperate-forest.*` |

To contribute another preset, add a schema-valid source record with its reviewed facts and complete source metadata, add one sorted index entry, run `build` and `check`, then add a single-pack story and its exact RON/JSON output. No Rust or shared module code changes are needed.

## Aotearoa New Zealand reviewed inputs

Every acquired file is named in `pack.weave-domain.json` with its URL, release or retrieval revision, SHA-256, license, attribution, and transformation. The checked hashes are:

| Input | Revision | License | SHA-256 |
|---|---|---|---|
| [Natural Earth 110m land](https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/110m_physical/ne_110m_land.shp) | `v5.1.2` | [Public domain](https://www.naturalearthdata.com/about/terms-of-use/) | `8689e6932b8e370e2ca4587cf3ba21e460b1235db37b6ed3c172c35b4a6088de` |
| [Natural Earth 110m ocean](https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/110m_physical/ne_110m_ocean.shp) | `v5.1.2` | [Public domain](https://www.naturalearthdata.com/about/terms-of-use/) | `8006f16c5af40875fbe8e975bef6d4b6c1ffc7b767491bdc277fbc0b62ba5d02` |
| [Earth Sciences New Zealand / NIWA mean monthly rainfall](https://niwa.co.nz/sites/default/files/inline-images/mean_monthly_rainfall_1991-2020_1.xlsx) | `1991-2020` normals, retrieved 2026-08-21 | [CC BY 4.0](https://niwa.co.nz/climate-and-weather/climate-data-and-activities#reuse) | `cff6bd6e4c4730850b744fe11b40a5867ad0efccee14a25adcffc6627174030b` |
| [Earth Sciences New Zealand / NIWA mean monthly air temperature](https://niwa.co.nz/sites/default/files/inline-images/mean_monthly_air_temperature_1991-2020_1.xlsx) | `1991-2020` normals, retrieved 2026-08-21 | [CC BY 4.0](https://niwa.co.nz/climate-and-weather/climate-data-and-activities#reuse) | `671feaf14025d344e421cd697c7a0338e0c7b332dc11d2c24a2107d07113a11a` |
| [RESOLVE Ecoregions 2017 country-envelope query](https://services.arcgis.com/P3ePLMYs2RVChkJx/arcgis/rest/services/Resolve_Ecoregions/FeatureServer/0/query?where=1%3D1&geometry=166%2C-48%2C179%2C-34&geometryType=esriGeometryEnvelope&inSR=4326&spatialRel=esriSpatialRelIntersects&outFields=ECO_NAME%2CBIOME_NAME%2CECO_BIOME_&returnGeometry=false&orderByFields=ECO_NAME&f=json) | `2017`, retrieved 2026-08-21 | [CC BY 4.0](https://developers.google.com/earth-engine/datasets/catalog/RESOLVE_ECOREGIONS_2017#terms-of-use) | `1fe165b148ad6936060ad8b7207dcd8a861f4c35f16c406fa119e5f62f7e6681` |

The climate transformation uses the published `YEAR` column for each in-scope station. It excludes the workbook's explicitly out-of-scope `Antarctica, Scott Base` row, producing a selected-station annual mean air-temperature range of `8.8–16.0 °C` and annual rainfall range of `365.1–6545.1 mm`. Cross-station monthly means identify January and February as the warmest months and July as the coolest. These are representative country-scale cues, not a forecast and not exhaustive local climate coverage.

The three representative-grid-point records retain exact NASA POWER Climatology API v2.9.7 URLs, access dates, response hashes, source-native MERRA-2 resolution, Celsius and `mm/day` inputs, and requested NASA attribution. Their RESOLVE envelope queries and Natural Earth vector files likewise retain exact hashes and open terms in each generated pack. See the [Weave World guide](../../../docs/world_module.md) for the complete normalization and failure policies.
