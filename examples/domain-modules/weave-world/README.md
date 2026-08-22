# Weave World reference-place fixture

This fixture is the first complete `org.weave.world` path. The source selects the exact `aotearoa_new_zealand@=1.0.0` pack, and that pack materializes one closed, deterministic `WorldSeed`. It is environmental reference material for fictional authoring: `culture_included` is explicitly `false`, and the pack makes no claims about people, language, identity, or culture.

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

## Reviewed public inputs

Every acquired file is named in `pack.weave-domain.json` with its URL, release or retrieval revision, SHA-256, license, attribution, and transformation. The checked hashes are:

| Input | Revision | License | SHA-256 |
|---|---|---|---|
| [Natural Earth 110m land](https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/110m_physical/ne_110m_land.shp) | `v5.1.2` | [Public domain](https://www.naturalearthdata.com/about/terms-of-use/) | `8689e6932b8e370e2ca4587cf3ba21e460b1235db37b6ed3c172c35b4a6088de` |
| [Natural Earth 110m ocean](https://raw.githubusercontent.com/nvkelso/natural-earth-vector/v5.1.2/110m_physical/ne_110m_ocean.shp) | `v5.1.2` | [Public domain](https://www.naturalearthdata.com/about/terms-of-use/) | `8006f16c5af40875fbe8e975bef6d4b6c1ffc7b767491bdc277fbc0b62ba5d02` |
| [Earth Sciences New Zealand / NIWA mean monthly rainfall](https://niwa.co.nz/sites/default/files/inline-images/mean_monthly_rainfall_1991-2020_1.xlsx) | `1991-2020` normals, retrieved 2026-08-21 | [CC BY 4.0](https://niwa.co.nz/climate-and-weather/climate-data-and-activities#reuse) | `cff6bd6e4c4730850b744fe11b40a5867ad0efccee14a25adcffc6627174030b` |
| [Earth Sciences New Zealand / NIWA mean monthly air temperature](https://niwa.co.nz/sites/default/files/inline-images/mean_monthly_air_temperature_1991-2020_1.xlsx) | `1991-2020` normals, retrieved 2026-08-21 | [CC BY 4.0](https://niwa.co.nz/climate-and-weather/climate-data-and-activities#reuse) | `671feaf14025d344e421cd697c7a0338e0c7b332dc11d2c24a2107d07113a11a` |
| [RESOLVE Ecoregions 2017 country-envelope query](https://services.arcgis.com/P3ePLMYs2RVChkJx/arcgis/rest/services/Resolve_Ecoregions/FeatureServer/0/query?where=1%3D1&geometry=166%2C-48%2C179%2C-34&geometryType=esriGeometryEnvelope&inSR=4326&spatialRel=esriSpatialRelIntersects&outFields=ECO_NAME%2CBIOME_NAME%2CECO_BIOME_&returnGeometry=false&orderByFields=ECO_NAME&f=json) | `2017`, retrieved 2026-08-21 | [CC BY 4.0](https://developers.google.com/earth-engine/datasets/catalog/RESOLVE_ECOREGIONS_2017#terms-of-use) | `1fe165b148ad6936060ad8b7207dcd8a861f4c35f16c406fa119e5f62f7e6681` |

The climate transformation uses the published `YEAR` column for each in-scope station. It excludes the workbook's explicitly out-of-scope `Antarctica, Scott Base` row, producing a selected-station annual mean air-temperature range of `8.8–16.0 °C` and annual rainfall range of `365.1–6545.1 mm`. Cross-station monthly means identify January and February as the warmest months and July as the coolest. These are representative country-scale cues, not a forecast and not exhaustive local climate coverage.
